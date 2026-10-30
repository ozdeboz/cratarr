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
  import { deckNow, local } from './localPlayer.svelte';
  import { isLocal } from './output.svelte';
  import { port } from './player/port.svelte';
  import { prefs } from './prefs.svelte';
  import { aimPainted, holdNow, ramping, tapeDir } from './ramping.svelte';
  import { reducedMotion } from './motion';
  import { portal } from './portal';
  import { art, clockText } from './remote.svelte';
  import type { Seek } from './seekState.svelte';
  import Waveform from './Waveform.svelte';
  import WaveLens from './WaveLens.svelte';
  import { loadWaveform, waveformOf } from './waveforms.svelte';
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
  // A hold's zoom on the playhead (the owner, 2026-10-28): the stretch the tape covers in ZOOM_S at its speed, at least
  // ZOOM_MIN_S, at most the whole: in as it winds up, out again as it gets faster, back to the whole when it ends; eased.
  // The playhead keeps the place it had on the bar when the hold began, the track sliding under it.
  const ZOOM_S = 15;
  const ZOOM_MIN_S = 20;
  const ZOOM_EASE_MS = 250;
  /** Out (back toward the whole) slower, for the eye to follow where it is (the owner, 2026-10-28). */
  const ZOOM_OUT_MS = 700;
  /** At its top speed this long, the whole track shows (the owner, 2026-10-28). */
  const ZOOM_TOP_MS = 1500;
  /** Let go, the zoom stays this long before it eases back to the whole (the owner, 2026-10-28). */
  const ZOOM_KEEP_MS = 2000;
  let zoomShare = $state(1);
  let zoomAnchor = $state<number | null>(null);
  const zoomOn = $derived(holdNow.on && port().holdsQueue && !reducedMotion());
  // Started and stopped by the hold alone (its frames move it, never restart it).
  $effect(() => {
    const on = zoomOn;
    return untrack(() => {
      if (!on && zoomShare >= 0.999) return;
      // A hold's start: the playhead's place on the bar as it is now (in the zoom still shown, if one is), held there.
      if (on && dur) {
        const p = Math.min(1, Math.max(0, pos / dur));
        zoomAnchor = view ? Math.min(1, Math.max(0, (p - view.from) / Math.max(1e-6, view.to - view.from))) : p;
      }
      let raf = 0;
      let last = performance.now();
      const offAt = on ? null : last;
      let topSince: number | null = null;
      const step = (t: number) => {
        // At its top speed a while: out to the whole.
        topSince = zoomOn && holdNow.speed >= holdNow.top * 0.97 ? (topSince ?? t) : null;
        const atTop = topSince !== null && t - topSince >= ZOOM_TOP_MS;
        const target = zoomOn && dur ? (atTop ? 1 : Math.min(1, Math.max(ZOOM_MIN_S, holdNow.speed * ZOOM_S) / dur)) : offAt !== null && t - offAt < ZOOM_KEEP_MS ? zoomShare : 1;
        zoomShare += (target - zoomShare) * (1 - Math.exp(-(t - last) / (target > zoomShare ? ZOOM_OUT_MS : ZOOM_EASE_MS)));
        last = t;
        if (!zoomOn && zoomShare > 0.999) {
          zoomShare = 1;
          zoomAnchor = null;
          return;
        }
        raf = requestAnimationFrame(step);
      };
      raf = requestAnimationFrame(step);
      return () => cancelAnimationFrame(raf);
    });
  });
  // The playing track's place and length as the player has them, in the same moment as which track it is (the props
  // follow a frame later: crossing a boundary, the strip was made a frame with the last track's, the owner 2026-10-28).
  const hereLen = $derived(local.queue[local.index]?.duration_secs ?? dur);
  const herePos = $derived(port().position() ?? pos);
  // Continuous (the owner, 2026-10-28): a hold's look-ahead in another track zooms out past the track's edge until it
  // shows, with as much again beyond it as it is ahead of the playhead; the tracks there drawn with their own waveforms,
  // their boundaries marked below. In s along the queue from this track's start (`a` to `b`).
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
  // A continuous hold runs on the strip throughout (the owner, 2026-10-28: its zoom moves on through a track change, no
  // track change of its own; only the cover and the background change as the next track plays): its window in s along
  // the queue. The playhead its focal point (the owner, 2026-10-28): where it was on the bar as the hold began, the queue
  // sliding under it; only the window's width goes with the speed, wide enough for the look-ahead in another track and
  // as much again beyond it.
  /** The playhead's place in the window (0..1). */
  const anchorOf = () => Math.min(0.9, Math.max(0.1, zoomAnchor ?? 0.5));
  /** Whether a hold winds on through the queue: Start and stop's as the player that winds has it, the app's in the app
   *  (the owner, 2026-10-29: the zoom's other tracks and their boundaries never showed there). */
  const throughQueue = $derived(startStopNow().edges === 'queue');
  const wideTarget = $derived.by((): { W: number } | null => {
    // Until it is let go (the owner, 2026-10-28): then back to this track's own view.
    if (!zoomOn || !holdNow.held || !throughQueue || !hereLen) return null;
    let W = Math.min(hereLen, Math.max(ZOOM_MIN_S, holdNow.speed * ZOOM_S));
    const li = holdNow.landingIndex;
    const l = holdNow.landing;
    const A = li !== null && l !== null && li !== local.index ? along(li, l) : null;
    if (A !== null) {
      const G = Math.abs(A - herePos);
      W = Math.max(W, A > herePos ? (2 * G) / (1 - anchorOf()) : (2 * G) / anchorOf());
    }
    return { W };
  });
  /** The wide view as shown, eased toward its target and back to the track's own view; null: none. */
  let wide = $state<{ a: number; b: number } | null>(null);
  const WIDE_EASE_MS = 400;
  $effect(() => {
    const on = wideTarget !== null;
    return untrack(() => {
      if (!on && !wide) return;
      let raf = 0;
      let last = performance.now();
      let idx = local.index;
      let len = hereLen ?? 0;
      const step = (t: number) => {
        const before = len;
        len = hereLen ?? 0;
        const own = view ? { a: view.from * len, b: view.to * len } : { a: 0, b: len };
        const e = 1 - Math.exp(-(t - last) / WIDE_EASE_MS);
        last = t;
        // Held: the width eased, the window placed on the playhead each frame (no lag behind it, however fast).
        if (wideTarget) {
          const was = wide ? wide.b - wide.a : own.b - own.a;
          const W = was + (wideTarget.W - was) * e;
          const a = herePos - anchorOf() * W;
          wide = { a, b: a + W };
          idx = local.index;
          raf = requestAnimationFrame(step);
          return;
        }
        // Let go, back to the track's own view: the tape into the next track or the one before keeps it where it was.
        if (wide && local.index !== idx) {
          if (local.index === idx + 1) wide = { a: wide.a - before, b: wide.b - before };
          else if (local.index === idx - 1 && hereLen) wide = { a: wide.a + hereLen, b: wide.b + hereLen };
          else wide = null;
          idx = local.index;
        }
        const w = wide ?? own;
        const next = { a: w.a + (own.a - w.a) * e, b: w.b + (own.b - w.b) * e };
        if (Math.abs(next.a - own.a) + Math.abs(next.b - own.b) < 0.005 * Math.max(1, own.b - own.a)) {
          wide = null;
          return;
        }
        wide = next;
        raf = requestAnimationFrame(step);
      };
      raf = requestAnimationFrame(step);
      return () => cancelAnimationFrame(raf);
    });
  });
  /** The tracks the wide view covers, by queue place, each from where along the queue. */
  const wideTracks = $derived.by(() => {
    if (!wide || !hereLen) return null;
    const out: { i: number; start: number; d: number }[] = [{ i: local.index, start: 0, d: hereLen }];
    // Not along the queue: this track alone.
    if (!throughQueue) return out;
    for (let i = local.index - 1, s = 0; i >= 0 && s > wide.a; i--) {
      const d = local.queue[i]?.duration_secs;
      if (!d) break;
      s -= d;
      out.unshift({ i, start: s, d });
    }
    for (let i = local.index + 1, s = hereLen; i < local.queue.length && s < wide.b; i++) {
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
  /** Which tracks those are: the strip is made again only when they change (not each frame the view eases). */
  const wideSet = $derived(wideTracks ? wideTracks.map((t) => `${t.i}:${t.start}:${t.d}`).join(',') : null);
  /** The tracks' waveforms end to end (each against its own loudest, at one resolution along the queue). */
  const wideBins = $derived.by(() => {
    const key = wideSet;
    if (!key || !hereLen) return null;
    const ts = key.split(',').map((k) => {
      const [i, start, d] = k.split(':').map(Number);
      return { i, start, d };
    });
    const lo = ts[0].start;
    const hi = ts[ts.length - 1].start + ts[ts.length - 1].d;
    const total = hi - lo;
    // One resolution along the queue (each track's bars the same whatever else the window covers).
    const per = 8;
    const parts = ts.map((t) => {
      const q = local.queue[t.i];
      const own = q ? waveformOf(q.source, q.id) : null;
      const m = Math.max(1, Math.round(t.d * per));
      const out = new Uint8Array(m);
      if (own && own.length) {
        let top = 1;
        for (const v of own) top = Math.max(top, v);
        for (let j = 0; j < m; j++) {
          const from = Math.floor((j * own.length) / m);
          const to = Math.max(from + 1, Math.floor(((j + 1) * own.length) / m));
          let v = 0;
          for (let k = from; k < to; k++) v = Math.max(v, own[k]);
          out[j] = Math.round((v / top) * 255);
        }
      }
      return out;
    });
    const all = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
    let o = 0;
    for (const p of parts) {
      all.set(p, o);
      o += p.length;
    }
    return { bins: all, lo, total, marks: ts.slice(1).map((t) => t.start), tracks: ts.map((t) => t.i) };
  });
  // Each track's colour in a wide view, its own (the owner, 2026-10-29: "colors should stay true to their waveform"):
  // the playing one's as the player has it, kept once it is the track before; the next ones' covers fetched ahead, so
  // their colours are there before a hold reaches them.
  $effect(() => {
    const t = local.queue[local.index];
    if (t) keepAccent(t, art.accent);
  });
  $effect(() => {
    for (let i = Math.max(0, local.index - 1); i <= local.index + 3; i++) {
      const t = local.queue[i];
      if (t) untrack(() => accentFor(t));
    }
  });
  /** The wide view for lib/Waveform: the strip, the stretch shown, this track's place in it and the boundaries. */
  const wideStrip = $derived.by(() => {
    const s = wideBins;
    if (!wide || !s || !hereLen) return null;
    const at = (x: number) => (x - s.lo) / s.total;
    return {
      bins: s.bins,
      from: Math.max(0, at(wide.a)),
      to: Math.min(1, at(wide.b)),
      cur0: at(0),
      cur1: at(hereLen),
      marks: s.marks.map(at),
      colours: s.tracks.map((i) => (local.queue[i] ? accentFor(local.queue[i]) : null)),
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
        target: !!wideTarget,
        wide: !!wide,
        tracks: wideTracks?.length ?? null,
        strip: !!wideBins,
      });
    wideLost = lost;
  });
  const view = $derived.by(() => {
    if (zoomShare >= 0.999 || zoomAnchor === null || !dur) return null;
    const p = Math.min(1, Math.max(0, pos / dur));
    const from = Math.min(1 - zoomShare, Math.max(0, p - zoomAnchor * zoomShare));
    return { from, to: from + zoomShare };
  });
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
    return (holdNow.dir > 0 ? pos < l : pos > l) ? l / dur : null;
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
    <Waveform bins={shaped ? (bins ?? null) : null} want={shownWant} {loading} progress={wideStrip && hereLen ? herePos / hereLen : dur ? pos / dur : 0} paused={grey} now={nowMark} {heard} {heardKnown} view={wideStrip ? null : view} wide={wideStrip} {track} left={isLocal() ? (local.left?.share ?? null) : null} back={isLocal() && ((holdNow.on && holdNow.dir < 0) || tapeDir.state === 'backward' || deckNow.back)} ahead={isLocal() && prefs.waveLevel === 'difference'} landing={aimMark} output={prefs.waveLevel === 'output'} onflat={() => (keepWave = false)} />
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
