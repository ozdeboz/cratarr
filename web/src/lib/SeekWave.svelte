<script lang="ts">
  // The track's position to seek (decision 0019), the same control in the full player and the mini player (the owner,
  // 2026-10-26): its waveform where it has one (lib/Waveform; while it loads, the line getting its gaps), else Finamp's
  // plain bar with its handle; held, the lens over the waveform (lib/WaveLens) and where it plays marked; let go, the
  // player seeks (lib/seekState's Seek holds all of that). While held, a big Cancel at the top of the screen, straight
  // above the waveform's middle: let go over it, nothing is sought (the owner, 2026-10-26).
  // `grab(e)`: the mini player's long press hands the pointer here, held from where it is.
  import { untrack } from 'svelte';
  import Icon from './Icon.svelte';
  import { accentFor, keepAccent } from './accents.svelte';
  import { appInfo, inApp } from './app.svelte';
  import { startStopNow } from './device.svelte';
  import { deckNow, local, throughOf } from './localPlayer.svelte';
  import { isLocal } from './output.svelte';
  import { port } from './player/port.svelte';
  import { prefs } from './prefs.svelte';
  import { aimPainted, holdNow, ramping, tapeDir } from './ramping.svelte';
  import { reducedMotion } from './motion';
  import { portal } from './portal';
  import { art, clockText, playingKey, shownCover, shownPlaying } from './remote.svelte';
  import type { Seek } from './seekState.svelte';
  import Waveform from './Waveform.svelte';
  import WaveLens from './WaveLens.svelte';
  import { loadWaveform, waveformOf } from './waveforms.svelte';
  import { stepWindow, waveColours, type HoldWindow } from './waveView';
  import { STRIP_SPAN, type StripTrack } from './waveStrip';
  import { playback } from './deviceLog';

  let {
    seek,
    bins,
    loading = null,
    dur,
    canSeek,
    paused = false,
    estimated = false,
    room,
    track = null,
    onend,
    ownColours = true,
  }: {
    seek: Seek;
    /** The waveform: undefined while it loads, null where the track has none. */
    bins: Uint8Array | null | undefined;
    /** How far it has loaded, 0..1, while it loads. */
    loading?: number | null;
    dur: number | null;
    canSeek: boolean;
    paused?: boolean;
    estimated?: boolean;
    /** The player's edges, for the lens. */
    room?: HTMLElement;
    /** Which track it shows (source/id): its waveform's pieces coloured by the track each shows. */
    track?: string | null;
    /** Let go (sought or not). */
    onend?: () => void;
    /** Whether the player's colour is its cover's (the mini player's Cover colour): a wide view's other tracks then
     *  theirs; else all one. */
    ownColours?: boolean;
  } = $props();

  let el = $state<HTMLElement>();
  let cancelEl = $state<HTMLElement>();
  /** A waveform that shows something (a Bandcamp analysis whose parts did not decode gives bins of nothing: the plain
   *  line). It decides the handle or the lens and the white line; while held, as it was when the finger went down. */
  const shapedNow = $derived(!!bins && bins.some((v) => v > 0));
  const shaped = $derived(seek.shapedHeld ?? shapedNow);
  /** What the waveform should be (lib/Waveform): its bars, its line getting its gaps while it loads, or the line. */
  const want = $derived<'wave' | 'wait' | 'line'>(shaped ? 'wave' : bins === undefined && loading !== null ? 'wait' : 'line');
  /** Drawn as a waveform until it has gone back into the line (none for this track): then the plain bar. */
  let keepWave = $state(false);
  $effect(() => {
    if (want !== 'line') keepWave = true;
  });
  const pos = $derived(seek.pos);
  /** As heard: this device's player's level (0..1), the waveform's bars scaled by it (lib/Waveform `heard`), where Level
   *  the volume says to show it. */
  const level = $derived(port().level());
  const heard = $derived(level && prefs.waveLevel !== 'file' ? 10 ** (level.db / 20) : 1);
  /** The level shown is the track's own (not a guess while its gain comes). */
  const heardKnown = $derived(!level || prefs.waveLevel === 'file' || level.known);
  /** Shown as paused: from the moment a stop ramp begins, playing from the moment a start ramp begins (the owner,
   *  2026-10-26: the colour goes with the ramp, whenever the player says it paused). */
  // After a ramp, its way holds until the player says the same (an app reporting the pause a moment late: no flash).
  let after = $state<'out' | 'in' | null>(null);
  /** How long after its ramp the colour waits for the player to say it paused or plays. */
  const AFTER_WAIT_MS = 2500;
  $effect(() => {
    if (port().ramps && ramping.dir && ramping.effect) after = ramping.dir;
  });
  $effect(() => {
    if ((after === 'out' && paused) || (after === 'in' && !paused)) after = null;
  });
  // Its ramp over and the player never said the same (a wind-down that went on to the next track, not a pause; the
  // owner's tablet, 2026-10-27: it stayed grey while it played): what the player says, a moment later.
  $effect(() => {
    if (!after || ramping.dir) return;
    const t = setTimeout(() => (after = null), AFTER_WAIT_MS);
    return () => clearTimeout(t);
  });
  // Only an effect that stops the music greys it (the owner, 2026-10-27), not the tape's own stops (lib/ramping).
  const grey = $derived(port().ramps && ramping.dir && ramping.effect ? ramping.dir === 'out' : after ? after === 'out' : paused);
  // The playing track's place and length as the player has them, in the same moment as which track it is (the props
  // follow a frame later: crossing a boundary, the strip was made a frame with the last track's, the owner 2026-10-28).
  // One source for the window, the playhead and the crosshair alike (0.10.1).
  const hereLen = $derived(local.queue[local.index]?.duration_secs ?? dur);
  const herePos = $derived(port().position() ?? pos);
  /** Where queue track `index`, `secs` into it, is along the queue from this track's start (null: a length unknown). */
  function along(index: number, secs: number): number | null {
    const cur = local.index;
    if (!hereLen) return null;
    if (index === cur) return secs;
    if (index > cur) {
      let x = hereLen;
      for (let i = cur + 1; i < index; i++) {
        const d = local.queue[i]?.duration_secs;
        if (!d) return null;
        x += d;
      }
      return x + secs;
    }
    let x = 0;
    for (let i = cur - 1; i > index; i--) {
      const d = local.queue[i]?.duration_secs;
      if (!d) return null;
      x -= d;
    }
    const d = local.queue[index]?.duration_secs;
    return d ? x - (d - secs) : null;
  }
  /** Whether a hold winds on through the queue: Start and stop's as the player that winds has it, the app's in the app
   *  (the owner, 2026-10-29: the zoom's other tracks and their boundaries never showed there). */
  const throughQueue = $derived(startStopNow().edges === 'queue');
  const zoomOn = $derived(holdNow.on && port().holdsQueue && !reducedMotion());
  /** Through the queue, how far a hold can go, from this track's start: the start of the first track it can wind back
   *  into and the end of the last it can wind on into, as the tape goes (lib/localPlayer's throughOf: no further than
   *  the queue's ends, a track it cannot wind or whose length is not known; repeat or not, a hold does not wrap). The
   *  window's walls (lib/waveView): no empty stretch before the queue or after it (0.12.3, the owner). */
  const queueEnds = $derived.by(() => {
    if (!throughQueue || local.index < 0 || !hereLen) return undefined;
    const back = throughOf(local.index, -1).at(-1) ?? local.index;
    const on = throughOf(local.index, 1).at(-1) ?? local.index;
    const lo = along(back, 0);
    const end = on === local.index ? hereLen : local.queue[on]?.duration_secs;
    const hi = end ? along(on, end) : null;
    return { lo: lo ?? -Infinity, hi: hi ?? Infinity };
  });
  // A hold's window (lib/waveView's stepWindow; 0.10.1, the owner: "So pure continuous transitions"): centred on the
  // crosshair, against the edge a direction does not need, as wide as the speed asks and standing once at the top
  // speed; let go, back to the track's own view. In s from the playing track's start. Through the queue it is the wide
  // view (the tracks around drawn with their own waveforms, their boundaries marked); on one track, its zoom.
  let win = $state<HoldWindow | null>(null);
  /** The track a hold's window began in: its start is where the wide view's places count from while the window lasts,
   *  so they stand still as the tape goes on into other tracks (0.11.1: counted from the playing track, every track
   *  change moved them all, and the bars drawn on them with it). */
  let holdBase = $state<number | null>(null);
  /** What shows, in s of the hold's frame (from holdBase's start), as the window's frame put it: the same place in the
   *  music whichever track plays when it is drawn (0.12, the owner: "bars jump sideways"; drawn from the window in the
   *  playing track's s, a track change showed a frame of the last track's window over the new track's places, a
   *  track's length off). */
  // The playhead too (`pos`), from the same frame: read as the page draws, it was a frame on from the window at times
  // and jittered against the bars.
  let holdShown = $state<{ from: number; to: number; pos: number } | null>(null);
  $effect(() => {
    const on = zoomOn && holdNow.held;
    return untrack(() => {
      if (!on && !win) return;
      if (!win) holdBase = local.index;
      let raf = 0;
      let last = performance.now();
      let idx = local.index;
      const step = (t: number) => {
        const dt = Math.max(0, Math.min(64, t - last));
        last = t;
        // Where the last frame's track's start is from this one's.
        const moved = local.index === idx ? 0 : along(idx, 0);
        idx = local.index;
        const li = holdNow.landingIndex ?? local.index;
        const l = holdNow.landing;
        win = stepWindow(
          win,
          {
            len: hereLen ?? 0,
            pos: herePos,
            moved,
            holding: zoomOn && holdNow.held,
            through: throughQueue,
            dir: holdNow.dir,
            speed: holdNow.speed,
            top: holdNow.top,
            aim: l === null ? null : along(li, l),
            ends: queueEnds,
          },
          dt,
        );
        const fa = win && holdBase !== null && holdBase !== local.index ? (along(holdBase, 0) ?? 0) : 0;
        holdShown = win ? { from: win.shown.from - fa, to: win.shown.to - fa, pos: herePos - fa } : null;
        if (win) raf = requestAnimationFrame(step);
      };
      raf = requestAnimationFrame(step);
      return () => cancelAnimationFrame(raf);
    });
  });
  /** Where the hold's frame counts from: the start of the track it began in, from this track's start (lib/waveStrip). */
  const frameAt = $derived(holdBase !== null && holdBase !== local.index ? (along(holdBase, 0) ?? 0) : 0);
  /** What shows in s from this track's start, from the hold's frame as it is drawn (holdShown). */
  const shownHere = $derived(win && holdShown ? { a: holdShown.from + frameAt, b: holdShown.to + frameAt } : null);
  /** The wide view: the window through the queue, in s from this track's start. */
  const wide = $derived(throughQueue ? shownHere : null);
  /** The tracks the wide view's strips cover (the window and as much again either side, lib/waveStrip's STRIP_SPAN), by
   *  queue place, each from where along the queue. */
  const wideTracks = $derived.by(() => {
    if (!wide || !hereLen) return null;
    const out: { i: number; start: number; d: number }[] = [{ i: local.index, start: 0, d: hereLen }];
    // Not along the queue: this track alone.
    if (!throughQueue) return out;
    const reach = ((STRIP_SPAN - 1) / 2) * (wide.b - wide.a);
    for (let i = local.index - 1, s = 0; i >= 0 && s > wide.a - reach; i--) {
      const d = local.queue[i]?.duration_secs;
      if (!d) break;
      s -= d;
      out.unshift({ i, start: s, d });
    }
    for (let i = local.index + 1, s = hereLen; i < local.queue.length && s < wide.b + reach; i++) {
      const d = local.queue[i]?.duration_secs;
      if (!d) break;
      out.push({ i, start: s, d });
      s += d;
    }
    return out;
  });
  $effect(() => {
    for (const t of wideTracks ?? []) {
      const q = local.queue[t.i];
      if (q) loadWaveform(q.source, q.id);
    }
  });
  /** The tracks the wide view covers, each in the hold's frame with its own bins (each against its own loudest,
   *  lib/waveStrip): a track's bars the same whatever else the window covers, wherever the tape is. */
  const wideBins = $derived.by((): { tracks: StripTrack[]; queue: number[] } | null => {
    if (!wideTracks || !hereLen) return null;
    const tracks = wideTracks.map((t) => {
      const q = local.queue[t.i];
      return { key: q ? `${q.source}/${q.id}` : `#${t.i}`, start: t.start - frameAt, d: t.d, bins: q ? (waveformOf(q.source, q.id) ?? null) : null };
    });
    return { tracks, queue: wideTracks.map((t) => t.i) };
  });
  // Each track's colour in a wide view, its own (the owner, 2026-10-29: "colors should stay true to their waveform"):
  // the playing one's as the player has it, kept once it is the track before; the next ones' covers fetched ahead, so
  // their colours are there before a hold reaches them.
  // Only while the accent is the shown track's own: after a track change the cover before (and its accent) shows a
  // moment longer, and was kept as the new track's (0.10.1).
  $effect(() => {
    const t = local.queue[local.index];
    const np = shownPlaying();
    if (t && np && np.source === t.source && np.id === t.id && art.of === shownCover()) keepAccent(t, art.accent);
  });
  $effect(() => {
    for (let i = Math.max(0, local.index - 1); i <= local.index + 3; i++) {
      const t = local.queue[i];
      if (t) untrack(() => accentFor(t));
    }
  });
  /** The wide view for lib/Waveform, in s of the hold's frame: its tracks, the stretch shown, this track's place in it
   *  and the boundaries. */
  const wideStrip = $derived.by(() => {
    const s = wideBins;
    if (!wide || !s || !hereLen) return null;
    return {
      tracks: s.tracks,
      from: holdShown!.from,
      to: holdShown!.to,
      cur0: -frameAt,
      cur1: hereLen - frameAt,
      marks: s.tracks.slice(1).map((t) => t.start),
      // One colour for the track the player shows, in every state (the owner, 2026-10-30: "The color should be
      // stable"; it was its cover's here and the player's elsewhere, yellow while winding, green let go).
      colours: waveColours(
        s.queue.map((i) => (local.queue[i] ? `${local.queue[i].source}/${local.queue[i].id}` : null)),
        playingKey(shownPlaying()),
        ownColours,
        (k) => {
          const q = s.queue.map((i) => local.queue[i]).find((q) => q && `${q.source}/${q.id}` === k);
          return q ? accentFor(q) : null;
        },
      ),
    };
  });
  /** What it shows: a continuous hold's strip a waveform throughout, whatever the playing track's own is doing. */
  const shownWant = $derived<'wave' | 'wait' | 'line'>(wideStrip ? 'wave' : want);
  const waveShown = $derived(!!wideStrip || keepWave || want !== 'line');
  // A continuous hold's window lost before its release: said in the playback log, with why.
  let wideLost = false;
  $effect(() => {
    const should = zoomOn && holdNow.held && throughQueue;
    const lost = should && !wideStrip;
    if (lost && !wideLost)
      playback('warn', 'hold window lost', {
        index: local.index,
        dur,
        win: !!win,
        tracks: wideTracks?.length ?? null,
        strip: !!wideBins,
      });
    wideLost = lost;
  });
  /** The zoom on one track (0..1 of it). */
  const view = $derived(shownHere && !throughQueue && hereLen ? { from: shownHere.a / hereLen, to: shownHere.b / hereLen } : null);
  /** The look-ahead shown (0..1 of the track): a hold's landing, ahead of the playhead the way it goes; in a track
   *  further on through the queue, at this one's edge that way, until the tape is in that track (the owner, 2026-10-28:
   *  the line stood at the end, and on the next track at the last one's place). */
  const aimMark = $derived.by(() => {
    const l = holdNow.landing;
    if (l === null || !dur || !port().holdsQueue) return null;
    const at = holdNow.landingIndex;
    if (at !== null && at !== local.index) {
      // Where it is in the wide view; else at this track's edge that way.
      const A = wideStrip && hereLen ? along(at, l) : null;
      if (A !== null && hereLen) return A / hereLen;
      return (holdNow.dir > 0 ? at > local.index : at < local.index) ? (holdNow.dir > 0 ? 1 : 0) : null;
    }
    const p = win ? herePos : pos;
    const len = (win && hereLen) || dur;
    return (holdNow.dir > 0 ? p < l : p > l) ? l / len : null;
  });
  // Its place as painted (after the frame's update), for lib/localPlayer's check against where the tape plays on.
  $effect(() => {
    if (aimMark === null) return;
    const t = holdNow.landingIndex !== null ? local.queue[holdNow.landingIndex] : null;
    aimPainted.pos = holdNow.landing;
    aimPainted.track = t ? `${t.source}/${t.id}` : track;
    aimPainted.at = performance.now();
  });
  const nowMark = $derived(seek.held !== null && seek.lensOn && !seek.snapped && !seek.overCancel && shaped && dur ? seek.now() / dur : null);
  /** Where the Cancel button stands: above the waveform's middle, measured whenever it shows (never left at 0). */
  let cancelX = $state(0);
  $effect(() => {
    if (seek.held === null || !seek.lensOn || !el) return;
    const r = el.getBoundingClientRect();
    cancelX = r.left + r.width / 2;
  });

  const awayFrom = (e: PointerEvent, target: HTMLElement) => {
    const r = target.getBoundingClientRect();
    return Math.max(0, r.top - e.clientY, e.clientY - r.bottom);
  };
  function start(e: { clientX: number }) {
    if (!el) return;
    seek.begin(e.clientX, el, shapedNow, room);
  }
  function down(e: PointerEvent) {
    if (!canSeek) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    start(e);
  }
  /** The mini player's long press: held from here on, the pointer this control's. */
  export function grab(e: { pointerId: number; clientX: number }) {
    if (!canSeek || !el) return;
    try {
      el.setPointerCapture(e.pointerId);
    } catch {
      /* the finger already gone */
    }
    start(e);
  }
  function move(e: PointerEvent) {
    if (seek.held === null || !el?.hasPointerCapture(e.pointerId)) return;
    const c = cancelEl?.getBoundingClientRect();
    seek.overCancel = !!c && e.clientX >= c.left && e.clientX <= c.right && e.clientY >= c.top && e.clientY <= c.bottom;
    seek.moveTo(e.clientX, el, awayFrom(e, el));
  }
  function up(e: PointerEvent) {
    if (seek.held === null || !el?.hasPointerCapture(e.pointerId)) {
      seek.lens = null;
      return;
    }
    // Where it is held (the lens's fine moves), not straight under the finger.
    seek.moveTo(e.clientX, el, awayFrom(e, el));
    seek.end();
    onend?.();
  }
  function cancel() {
    seek.letGo();
    onend?.();
  }
  // The finger lost (the system took it, or this control was replaced while held): let go, never held for good.
  function lost() {
    if (seek.held !== null) cancel();
  }
  $effect(() => () => {
    if (seek.held !== null) seek.letGo();
  });
  function key(e: KeyboardEvent) {
    if (!canSeek || !dur) return;
    const now = seek.now();
    const to = e.key === 'ArrowLeft' ? now - 5 : e.key === 'ArrowRight' ? now + 5 : e.key === 'Home' ? 0 : e.key === 'End' ? dur - 1 : null;
    if (to === null) return;
    e.preventDefault();
    seek.release(Math.min(dur, Math.max(0, to)));
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  class="seek"
  class:can={canSeek}
  class:held={seek.held !== null}
  class:cancelling={seek.overCancel}
  class:paused={grey}
  class:estimated
  style:--ramp-ms={port().ramps && ramping.dir ? `${ramping.ms}ms` : '0ms'}
  bind:this={el}
  role={canSeek ? 'slider' : 'progressbar'}
  tabindex={canSeek ? 0 : undefined}
  aria-label={paused ? 'Track position, paused' : 'Track position'}
  aria-valuemin={0}
  aria-valuemax={dur ?? undefined}
  aria-valuenow={Math.floor(pos)}
  aria-valuetext={dur ? `${clockText(pos)} of ${clockText(dur)}` : clockText(pos)}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={cancel}
  onlostpointercapture={lost}
  onkeydown={key}
>
  {#if waveShown}
    <Waveform bins={shaped ? (bins ?? null) : null} want={shownWant} {loading} progress={shownHere && holdShown && hereLen ? (holdShown.pos + frameAt) / hereLen : dur ? pos / dur : 0} paused={grey} now={nowMark} {heard} {heardKnown} view={wideStrip ? null : view} wide={wideStrip} {track} left={isLocal() ? (local.left?.share ?? null) : null} back={isLocal() && ((holdNow.on && holdNow.dir < 0) || tapeDir.state === 'backward' || deckNow.back)} ahead={isLocal() && prefs.waveLevel === 'difference'} landing={aimMark} output={prefs.waveLevel === 'output'} onflat={() => (keepWave = false)} />
  {:else}
    <div class="bar"><div class="fill" style:transform="scaleX({dur ? pos / dur : 0})"></div></div>
  {/if}
  <!-- The plain bar's handle (no lens there, the owner 2026-10-25); over a waveform, while held, the lens instead. -->
  <!-- Only on the plain bar: not over a waveform waiting for the next track's (the owner, 2026-10-26: a circle flashed
       on Next between two known waveforms). -->
  {#if canSeek && !waveShown}<span class="thumb" style:left="{dur ? (pos / dur) * 100 : 0}%"></span>{/if}
  {#if bins && shaped && seek.lens && seek.lensOn && seek.held !== null && !seek.overCancel && dur}
    <WaveLens {bins} point={pos / dur} now={nowMark} {...seek.lens} time={clockText(pos)} {paused} zoom={seek.lensZoom} />
  {/if}
</div>
{#if seek.held !== null && seek.lensOn}
  <div use:portal>
    <span class="cancel" class:over={seek.overCancel} bind:this={cancelEl} style:left="{cancelX}px" role="img" aria-label="Let go here to cancel"
      ><Icon name="close" size={30} /></span
    >
  </div>
{/if}

<style>
  /* Finamp's slider: 24 px to hold, the track in the middle, a round handle when it can seek. */
  .seek {
    position: relative;
    flex: 1;
    display: flex;
    align-items: center;
    height: 24px;
  }

  .seek.can {
    cursor: pointer;
    touch-action: none;
  }

  .seek:focus-visible {
    outline: 2px solid var(--art-accent);
    outline-offset: 2px;
    border-radius: 4px;
  }

  .thumb {
    position: absolute;
    top: 50%;
    width: 14px;
    height: 14px;
    margin: -7px 0 0 -7px;
    border-radius: 50%;
    background: var(--art-accent);
    pointer-events: none;
  }

  .seek.held .thumb {
    transform: scale(1.3);
  }

  /* Over Cancel: back where it plays, dimmed, no lens: let go, nothing changes. */
  .seek.cancelling {
    opacity: 0.45;
    transition: opacity 120ms ease-out;
  }

  .bar {
    flex: 1;
    height: 4px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--text) 18%, transparent);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    background: var(--art-accent);
    transform-origin: left;
  }

  /* Paused, what has played loses its colour: over a Start and stop ramp, in step with it (the owner, 2026-10-26). */
  .fill {
    transition: background var(--ramp-ms, 0ms) linear;
  }

  .paused .fill {
    background: var(--muted);
  }

  .estimated .fill {
    opacity: 0.5;
  }

  /* Cancel, at the top of the screen above the waveform: big, the danger's colour once the finger is over it. */
  .cancel {
    position: fixed;
    top: calc(env(safe-area-inset-top, 0px) + 24px);
    z-index: 1000;
    display: grid;
    place-items: center;
    width: 72px;
    height: 72px;
    margin-left: -36px;
    border-radius: 50%;
    background: color-mix(in srgb, var(--surface) 85%, transparent);
    color: var(--text);
    box-shadow: 0 4px 16px rgb(0 0 0 / 0.35);
    backdrop-filter: blur(8px);
    pointer-events: none;
    transition:
      transform 120ms ease-out,
      background 120ms ease-out;
  }

  .cancel.over {
    background: var(--danger);
    color: #fff;
    transform: scale(1.15);
  }
</style>
