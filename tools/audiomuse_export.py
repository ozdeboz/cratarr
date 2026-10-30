"""AudioMuse's analysis for Cratarr (decision 0054): what an AudioMuse-AI database holds for the tracks it matched to
Cratarr's own (`provider_track_id` `tr-…`), as the JSON lines Cratarr takes in from `<data>/audiomuse/audiomuse.jsonl`.

Reads only (one SELECT through `psql` in AudioMuse's database container). Writes `--out`; with `--copy-to`
(`host:/path/audiomuse.jsonl`, an ssh host), copies it there and puts it in place in one move, and only when it
changed since the last copy (its SHA-256 is kept next to `--out`).

    python audiomuse_export.py --out audiomuse.jsonl --copy-to nas:/mnt/nvme/cratarr/data/audiomuse/audiomuse.jsonl \\
        --docker "wsl.exe -d Ubuntu -- docker"
"""
import argparse
import hashlib
import json
import os
import shlex
import subprocess
import sys

SEP = '\x1f'
# Run by a scheduled task with pythonw: the commands it runs open no windows either.
QUIET = {'creationflags': getattr(subprocess, 'CREATE_NO_WINDOW', 0)}
QUERY = """
SELECT m.provider_track_id, s.energy, s.key, s.scale, s.mood_vector, s.other_features,
       replace(encode(e.embedding, 'base64'), E'\\n', '')
FROM track_server_map m
JOIN score s ON s.item_id = m.item_id
LEFT JOIN embedding e ON e.item_id = m.item_id
WHERE m.provider_track_id LIKE 'tr-%'
ORDER BY m.provider_track_id
"""


def rows(docker: list[str], container: str, user: str, db: str) -> list[dict]:
    cmd = docker + ['exec', container, 'psql', '-U', user, '-d', db, '-At', '-F', SEP, '-c', QUERY]
    out = subprocess.run(cmd, capture_output=True, check=True, **QUIET).stdout.decode('utf-8')
    lines = []
    for raw in out.splitlines():
        f = raw.split(SEP)
        if len(f) != 7 or not f[0]:
            continue
        track, energy, key, scale, style, moods, embedding = f
        line = {'track': track}
        if energy:
            line['energy'] = round(float(energy), 4)
        for name, value in (('key', key), ('scale', scale), ('style', style), ('moods', moods), ('embedding', embedding)):
            if value:
                line[name] = value
        lines.append(line)
    return lines


def main() -> int:
    a = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    a.add_argument('--out', required=True, help='the JSON lines file to write')
    a.add_argument('--copy-to', help='host:/path to copy it to (over ssh), when it changed')
    a.add_argument('--docker', default='docker', help='the docker command (e.g. "wsl.exe -d Ubuntu -- docker")')
    a.add_argument('--container', default='audiomuse-postgres-1')
    a.add_argument('--user', default='audiomuse')
    a.add_argument('--db', default='audiomusedb')
    args = a.parse_args()

    lines = rows(shlex.split(args.docker), args.container, args.user, args.db)
    text = ''.join(json.dumps(l, separators=(',', ':')) + '\n' for l in lines)
    part = args.out + '.part'
    with open(part, 'w', encoding='utf-8', newline='\n') as f:
        f.write(text)
    os.replace(part, args.out)
    digest = hashlib.sha256(text.encode('utf-8')).hexdigest()
    print(f'{len(lines)} tracks written to {args.out}')
    if not args.copy_to:
        return 0
    last_file = args.out + '.copied'
    last = open(last_file, encoding='utf-8').read().strip() if os.path.exists(last_file) else ''
    if last == f'{digest} {args.copy_to}':
        print('unchanged since the last copy: not copied')
        return 0
    host, path = args.copy_to.split(':', 1)
    folder = path.rsplit('/', 1)[0]
    # A folder made here is open to read: Cratarr may run as another user than the one ssh signs in as.
    q = shlex.quote(folder)
    subprocess.run(['ssh', '-o', 'BatchMode=yes', host, f'[ -d {q} ] || {{ mkdir -p {q} && chmod 755 {q}; }}'], check=True, **QUIET)
    subprocess.run(['scp', '-q', '-o', 'BatchMode=yes', args.out, f'{host}:{path}.part'], check=True, **QUIET)
    subprocess.run(['ssh', '-o', 'BatchMode=yes', host, f'chmod 644 {shlex.quote(path + ".part")} && mv {shlex.quote(path + ".part")} {shlex.quote(path)}'], check=True, **QUIET)
    with open(last_file, 'w', encoding='utf-8') as f:
        f.write(f'{digest} {args.copy_to}\n')
    print(f'copied to {args.copy_to}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
