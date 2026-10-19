"""Beat this! over the library, on a PC (decision 0049, amended: "the library on the PC, new music on the NAS"; the
small model). The official `beat_this` package (pip install beat_this; PyTorch, soxr) hears each file Cratarr lists
in `<data>/beats/wanted.jsonl`, and what it heard goes back in batches to `<data>/beats/in/` over SSH, where Cratarr
takes it in. The audio is read over SSH too (`ssh <host> cat`) and decoded as the package's own loader does when
torchaudio cannot (soundfile; ffmpeg for what it cannot read either), at the file's own rate and channels: the package
makes it mono and resamples it.

Each line sent: `{"folder", "rel", "size", "mtime", "beats": [ms, …], "downbeats": [ms, …]}`, the file as Cratarr
listed it. What was sent is also kept in `--work` (`sent.jsonl`), so a run started again skips it; failures go to
`failed.jsonl` there.

    python beat_this_pass.py --data nas:/mnt/nvme/cratarr/data --map /media=/Volume1/media --work beat-this-work
"""
import argparse
import json
import os
import queue
import shlex
import subprocess
import sys
import threading
import time

BATCH = 100


def remote(host: str, command: str, **kw) -> subprocess.CompletedProcess:
    return subprocess.run(['ssh', '-o', 'BatchMode=yes', host, command], check=True, capture_output=True, **kw)


def key(item: dict) -> tuple:
    return (item['folder'], item['rel'], item['size'], item['mtime'])


def read_lines(path: str) -> list[dict]:
    if not os.path.exists(path):
        return []
    with open(path, encoding='utf-8') as f:
        return [json.loads(l) for l in f if l.strip()]


def decode(host: str, path: str):
    """The file's sound and its rate, every channel, as read over `ssh cat`: by soundfile (the package's loader when
    torchaudio cannot), else by ffmpeg at 22,050 Hz in two channels."""
    import io
    import numpy as np
    import soundfile as sf
    got = subprocess.run(['ssh', '-o', 'BatchMode=yes', host, f'cat {shlex.quote(path)}'], capture_output=True)
    if got.returncode != 0:
        raise RuntimeError(f'ssh cat ended with {got.returncode}: {got.stderr.decode(errors="replace").strip()[:200]}')
    try:
        sound, rate = sf.read(io.BytesIO(got.stdout), dtype='float32')
    except Exception:  # noqa: BLE001: soundfile cannot read it; ffmpeg can
        ff = subprocess.run(['ffmpeg', '-v', 'error', '-i', 'pipe:0', '-f', 'f32le', '-ac', '2', '-ar', '22050', 'pipe:1'],
                            input=got.stdout, capture_output=True)
        if ff.returncode != 0:
            raise RuntimeError(f'ffmpeg: {ff.stderr.decode(errors="replace").strip()[:300]}')
        sound, rate = np.frombuffer(ff.stdout, dtype=np.float32).reshape(-1, 2), 22050
    if len(sound) == 0:
        raise RuntimeError('no sound')
    return sound, rate


def main() -> int:
    a = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    a.add_argument('--data', required=True, help="host:/path of Cratarr's data folder (over ssh)")
    a.add_argument('--map', action='append', default=[], help='server path prefix = host path prefix (/media=/Volume1/media)')
    a.add_argument('--work', required=True, help='a local folder for what was sent and what failed')
    a.add_argument('--model', default='small0', help="beat_this's checkpoint (decision 0049: small0)")
    a.add_argument('--threads', type=int, default=8, help="PyTorch's threads")
    a.add_argument('--readers', type=int, default=3, help='files read and decoded at once')
    a.add_argument('--limit', type=int, default=0, help='at most this many files (0: all)')
    args = a.parse_args()

    import numpy as np
    import torch
    from beat_this.inference import Audio2Beats

    host, data = args.data.split(':', 1)
    maps = [m.split('=', 1) for m in args.map]
    os.makedirs(args.work, exist_ok=True)
    sent_file, failed_file = os.path.join(args.work, 'sent.jsonl'), os.path.join(args.work, 'failed.jsonl')
    done = {key(l) for l in read_lines(sent_file)} | {key(l) for l in read_lines(failed_file)}
    wanted = [json.loads(l) for l in remote(host, f'cat {shlex.quote(data + "/beats/wanted.jsonl")}').stdout.decode('utf-8').splitlines() if l.strip()]
    todo = [w for w in wanted if key(w) not in done]
    if args.limit:
        todo = todo[:args.limit]
    print(f'{len(wanted)} listed, {len(wanted) - len(todo)} done before, {len(todo)} to hear', flush=True)
    if not todo:
        return 0

    def host_path(server_path: str) -> str:
        for frm, to in maps:
            if server_path == frm or server_path.startswith(frm.rstrip('/') + '/'):
                return to.rstrip('/') + server_path[len(frm.rstrip('/')):]
        return server_path

    torch.set_num_threads(args.threads)
    hear = Audio2Beats(checkpoint_path=args.model, device='cpu', dbn=False)

    # Decoded sound waits here: at the file's own rate, a long mix is hundreds of MB.
    sounds: queue.Queue = queue.Queue(maxsize=args.readers)
    items: queue.Queue = queue.Queue()
    for w in todo:
        items.put(w)

    def reader():
        while True:
            try:
                w = items.get_nowait()
            except queue.Empty:
                sounds.put(None)
                return
            path = host_path(w['folder'].rstrip('/') + '/' + w['rel'])
            try:
                sounds.put((w, decode(host, path), None))
            except Exception as e:  # noqa: BLE001: one file's failure is noted, the pass goes on
                sounds.put((w, None, str(e)))

    for _ in range(args.readers):
        threading.Thread(target=reader, daemon=True).start()

    batch: list[dict] = []
    stamp = time.strftime('%Y%m%d-%H%M%S')
    sent_batches = 0

    def send():
        nonlocal batch, sent_batches
        if not batch:
            return
        name = f'{stamp}-{sent_batches:05d}.jsonl'
        local = os.path.join(args.work, 'batch.jsonl')
        text = ''.join(json.dumps(l, separators=(',', ':')) + '\n' for l in batch)
        with open(local, 'w', encoding='utf-8', newline='\n') as f:
            f.write(text)
        target = f'{data}/beats/in/{name}'
        subprocess.run(['scp', '-q', '-o', 'BatchMode=yes', local, f'{host}:{target}.part'], check=True)
        remote(host, f'chmod 644 {shlex.quote(target + ".part")} && mv {shlex.quote(target + ".part")} {shlex.quote(target)}')
        with open(sent_file, 'a', encoding='utf-8', newline='\n') as f:
            f.write(text)
        sent_batches += 1
        batch = []

    started, heard, failed, seconds, ended = time.time(), 0, 0, 0.0, 0
    while ended < args.readers:
        got = sounds.get()
        if got is None:
            ended += 1
            continue
        w, decoded, error = got
        if error is None:
            sound, rate = decoded
            try:
                beats, downbeats = hear(sound, rate)
                ms = lambda v: [int(round(float(t) * 1000)) for t in v]  # noqa: E731
                batch.append({**{k: w[k] for k in ('folder', 'rel', 'size', 'mtime')}, 'beats': ms(beats), 'downbeats': ms(downbeats)})
                heard += 1
                seconds += len(sound) / rate
            except Exception as e:  # noqa: BLE001
                error = f'beat_this: {e}'
        if error is not None:
            failed += 1
            with open(failed_file, 'a', encoding='utf-8', newline='\n') as f:
                f.write(json.dumps({**{k: w[k] for k in ('folder', 'rel', 'size', 'mtime')}, 'error': error}) + '\n')
        if len(batch) >= BATCH:
            send()
        n = heard + failed
        if n % 50 == 0:
            took = time.time() - started
            left = (len(todo) - n) * took / n
            print(f'{n}/{len(todo)}: {heard} heard ({seconds / 3600:.1f} h of sound, {seconds / took:.0f}x real time), {failed} failed; '
                  f'{left / 3600:.1f} h left', flush=True)
    send()
    print(f'done: {heard} heard, {failed} failed, in {(time.time() - started) / 3600:.2f} h', flush=True)
    return 0


if __name__ == '__main__':
    sys.exit(main())
