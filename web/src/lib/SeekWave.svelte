<script lang="ts">
  // The track's position to seek (decision 0019), the same control in the full player and the mini player (the owner,
  // 2026-10-26): its waveform where it has one (lib/Waveform; while it loads, the line getting its gaps), else Finamp's
  // plain bar with its handle; held, the lens over the waveform (lib/WaveLens) and where it plays marked; let go, the
  // player seeks (lib/seekState's Seek holds all of that). While held, a big Cancel at the top of the screen, straight
  // above the waveform's middle: let go over it, nothing is sought (the owner, 2026-10-26).
  // `grab(e)`: the mini player's long press hands the pointer here, held from where it is.
  import { untrack } from 'svelte';
  import Icon from './Icon.svelte';
  import { local } from './localPlayer.svelte';
  import { isLocal } from './output.svelte';
  import { prefs } from './prefs.svelte';
  import { holdNow, ramping, tapeDir } from './ramping.svelte';
  import { reducedMotion } from './motion';
  import { portal } from './portal';
  import { clockText } from './remote.svelte';
  import type { Seek } from './seekState.svelte';
  import Waveform from './Waveform.svelte';
  import WaveLens from './WaveLens.svelte';

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
  const waveShown = $derived(keepWave || want !== 'line');
  const pos = $derived(seek.pos);
  /** As heard: this device's player's level (0..1), the waveform's bars scaled by it (lib/Waveform `heard`), where Level
   *  the volume says to show it. */
  const heard = $derived(isLocal() && prefs.waveLevel !== 'file' ? 10 ** (local.levelDb / 20) : 1);
  /** The level shown is the track's own (not a guess while its gain comes). */
  const heardKnown = $derived(!isLocal() || prefs.waveLevel === 'file' || local.levelKnown);
  /** Shown as paused: from the moment a stop ramp begins, playing from the moment a start ramp begins (the owner,
   *  2026-10-26: the colour goes with the ramp, whenever the player says it paused). */
  // After a ramp, its way holds until the player says the same (an app reporting the pause a moment late: no flash).
  let after = $state<'out' | 'in' | null>(null);
  /** How long after its ramp the colour waits for the player to say it paused or plays. */
  const AFTER_WAIT_MS = 2500;
  $effect(() => {
    if (isLocal() && ramping.dir && ramping.effect) after = ramping.dir;
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
  const grey = $derived(isLocal() && ramping.dir && ramping.effect ? ramping.dir === 'out' : after ? after === 'out' : paused);
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
  const zoomOn = $derived(holdNow.on && isLocal() && !reducedMotion());
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
  const view = $derived.by(() => {
    if (zoomShare >= 0.999 || zoomAnchor === null || !dur) return null;
    const p = Math.min(1, Math.max(0, pos / dur));
    const from = Math.min(1 - zoomShare, Math.max(0, p - zoomAnchor * zoomShare));
    return { from, to: from + zoomShare };
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
  style:--ramp-ms={isLocal() && ramping.dir ? `${ramping.ms}ms` : '0ms'}
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
    <Waveform bins={shaped ? (bins ?? null) : null} {want} {loading} progress={dur ? pos / dur : 0} paused={grey} now={nowMark} {heard} {heardKnown} {view} {track} left={isLocal() ? (local.left?.share ?? null) : null} back={isLocal() && ((holdNow.on && holdNow.dir < 0) || tapeDir.state === 'backward')} ahead={isLocal() && prefs.waveLevel === 'difference'} landing={holdNow.landing !== null && dur && (holdNow.dir > 0 ? pos < holdNow.landing : pos > holdNow.landing) ? holdNow.landing / dur : null} output={prefs.waveLevel === 'output'} onflat={() => (keepWave = false)} />
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
