<script lang="ts">
  // The tape deck in the player's row under the title, while an effect runs (lib/NowPlaying; the owner, 2026-10-27):
  // its reels and wheels turn at the playing speed (a Vinyl ramp's speed, a rewind's backwards), its packs follow the
  // place in the track (lib/tapeDeck), and its head lights on each bar's first beat (lib/visuals/downbeat).
  // What turns turns on the compositor (the tablet's Brave, 2026-10-27: SVG drawn anew each frame ran at ~31 fps with
  // hitches of 50–80 ms, the reels visibly stuttering through a ramp): each turning part is its own small layer over the
  // still drawing, an endless turn whose rate alone is changed, and only when the speed has moved; the packs and the
  // tape four times a second; the head's light an opacity.
  import { appInfo, inApp } from './app.svelte';
  import { startStopNow } from './device.svelte';
  import { port } from './player/port.svelte';
  import { prefs } from './prefs.svelte';
  import { ramping, tapeDir } from './ramping.svelte';
  import { deckSpeed, turnTape, windable } from './localPlayer.svelte';
  import { drivenName, playOn } from './playOn.svelte';
  import { duration, playing, position, shownPaused } from './remote.svelte';
  import { hill, HEAD, HUB, MAXR, packs, REEL_L, REEL_R, tapePath, WHEELS } from './tapeDeck';
  import { travelOf } from './player/knob';
  import { downbeatAt } from './visuals/downbeat';

  /** Radians a second a wheel of radius 1 turns at 1× (as the mock looked right). */
  const TURN = 2.4 * Math.PI * 12;
  /** The fastest the picture turns, whatever the tape does (past it the spokes would strobe). */
  const MOST = 16;
  /** The drawing's own size (its viewBox): drawn at this size, scaled down where the row is narrower, and up on a large
   *  landscape screen (the owner, 2026-10-30: "the tape deck could probably be 300% size on landscape 1080p"). */
  const W = 320;
  const H = 43;
  /** A turn's length at rate 1 (ms): a part's rate is then its turns a second. */
  const TURN_MS = 1000;
  /** Where every turn starts: far enough in that turning backwards never runs out of turns (a whole number of them). */
  const FAR_IN = 1e9;
  /** How often the turning's rate may change (ms), and by how much it must have (a share of it). */
  const RATE_EVERY = 50;
  const RATE_STEP = 0.02;
  /** How often the packs and the tape follow the place in the track (ms). */
  const PLACE_EVERY = 250;
  /** The spokes' and the notches' boxes: half a side. */
  const SPOKES = HUB + 4;
  /** The head's box around its two hills. */
  const HEAD_BOX = { x: HEAD.x - HEAD.half - 1, y: HEAD.y - HEAD.gap - HEAD.height - 1, w: 2 * HEAD.half + 2, h: 2 * (HEAD.gap + HEAD.height) + 2 };

  /** How far it may grow: up to three times on a landscape screen at least as tall as 1080p's (a full HD window), its own
   *  size elsewhere. */
  const BIG = '(orientation: landscape) and (min-height: 900px)';
  const MOST_SCALE = 3;
  let big = $state(typeof window !== 'undefined' && matchMedia(BIG).matches);
  $effect(() => {
    const m = matchMedia(BIG);
    const on = () => (big = m.matches);
    m.addEventListener('change', on);
    return () => m.removeEventListener('change', on);
  });
  const most = $derived(big ? MOST_SCALE : 1);
  let width = $state(W);
  const scale = $derived(Math.min(most, width / W));

  let tape: SVGPathElement;
  let packL: SVGCircleElement;
  let packR: SVGCircleElement;
  let spokesL: HTMLDivElement;
  let spokesR: HTMLDivElement;
  let notches: HTMLDivElement[] = [];
  let lit: HTMLDivElement;

  /** The tape's speed now (× the track's; negative: backwards). */
  let speed = 0;
  /** The way the tape goes now, its L or R lit (directions are pure, 0.12.4; the owner, 2026-10-31: the deck's L and R
   *  did not turn while it went backwards): the knob's pointer shows how it is set, the letters which way it goes. */
  let travel = $state<'L' | 'R'>(tapeDir.want);
  let lastPos: number | null = null;
  let lastAt = 0;
  function speedNow(now: number, pos: number): number {
    const np = playing();
    const rewinding = port().rewinding();
    const mode = startStopNow().mode;
    const dt = Math.max(0.001, (now - lastAt) / 1000);
    const moved = lastPos === null ? 0 : (pos - lastPos) / dt;
    lastPos = pos;
    lastAt = now;
    // The app's deck says its speed (0.9.14-2): that, as it is.
    const said = port().holdsQueue ? deckSpeed(now) : null;
    if (said !== null) return said;
    if (rewinding) return speed * 0.85 + moved * 0.15;
    if (ramping.dir) {
      if (mode !== 'vinyl') return 1;
      const u = Math.min(1, Math.max(0, (now - ramping.at) / Math.max(1, ramping.ms)));
      // Down on the deck's one brake (a raised cosine, as lib/localPlayer's), up as the wind-up eases out.
      return ramping.dir === 'out' ? (1 + Math.cos(Math.PI * u)) / 2 : 1 - (1 - u) ** 2;
    }
    return shownPaused() ? 0 : 1;
  }

  function run(_: HTMLDivElement) {
    let radii = packs(0.5);
    /** Each turning part, its radius (a reel's pack changes with the place) and its endless turn. */
    const parts = [
      { el: spokesL, r: () => radii[0] },
      { el: spokesR, r: () => radii[1] },
      // A wheel turns the way the tape goes round it: the inner two, the tape over their other side, the other way (the
      // owner, 2026-10-27: they turned against it).
      ...WHEELS.map((w, i) => ({ el: notches[i], r: () => w.r, sense: -w.s })),
    ].map((p) => {
      const turn = p.el.animate([{ transform: 'rotate(0turn)' }, { transform: 'rotate(1turn)' }], { duration: TURN_MS, iterations: Infinity });
      turn.currentTime = FAR_IN;
      turn.playbackRate = 0;
      return { ...p, turn };
    });
    /** The speed the parts turn at now, and since when. */
    let shown = 0;
    let shownAt = -Infinity;
    const turnAt = (v: number) => {
      for (const p of parts) p.turn.updatePlaybackRate(((v * TURN) / p.r() / (2 * Math.PI)) * ('sense' in p ? (p.sense as number) : 1));
      shown = v;
    };
    let placedAt = -Infinity;
    let litShown = -1;
    let frame = 0;
    const tick = (now: number) => {
      const np = playing();
      const pos = position();
      speed = speedNow(now, pos);
      const v = Math.max(-MOST, Math.min(MOST, speed));
      const way = travelOf(v, travel);
      if (way !== travel) travel = way;
      if (now - placedAt >= PLACE_EVERY) {
        placedAt = now;
        const dur = duration();
        const place = dur ? Math.min(0.98, Math.max(0.02, pos / dur)) : 0.5;
        radii = packs(place);
        packL.setAttribute('r', radii[0].toFixed(2));
        packR.setAttribute('r', radii[1].toFixed(2));
        tape.setAttribute('d', tapePath(place));
        // The reels turn at their packs' new radii.
        turnAt(shown);
      }
      // Still and moving told at once; otherwise only a real change, at most every RATE_EVERY.
      if ((v === 0) !== (shown === 0) || (now - shownAt >= RATE_EVERY && Math.abs(v - shown) > RATE_STEP * Math.max(Math.abs(shown), 0.05))) {
        shownAt = now;
        turnAt(v);
      }
      // The head: lit on the downbeat, in the accent.
      const l = Math.round((np ? downbeatAt(np.source, np.id, pos) : 0) * 100) / 100;
      if (l !== litShown) {
        litShown = l;
        lit.style.opacity = String(l);
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(frame);
      for (const p of parts) p.turn.cancel();
    };
  }

  /** The direction knob, under the head between the inner wheels: its centre and size, and its L and R either side, tiny
   *  (the owner, 2026-10-27: the deck's proportions unchanged). */
  const KNOB = { x: 160, y: 36, r: 3.2 };
  /** The knob where a player can turn: this page's own, the app's, one driven (not Kodi, which plays as told). */
  const turnable = $derived(
    port().holdsQueue &&
      playOn.players.find((p) => p.name === drivenName())?.kind !== 'kodi' &&
      // Part of the tape engine: only while Tape winding is on (the owner, 2026-10-27).
      startStopNow().rewind,
  );
  /** Nothing to play backwards from (a Bandcamp preview, the device's own music): the knob dimmed, still. */
  const stuck = $derived(!windable(playing()));
  /** The knob as a machine's (the owner, 2026-10-28: a chicken-head): a tapering bright pointer over a dark skirt,
   *  turned SWEEP degrees to the side it is at; its direction reads even at its true size. */
  const SWEEP = 50;
  /** A layer's box over the drawing: around (x, y), `half` either side. */
  const box = (x: number, y: number, half: number) => `left:${x - half}px;top:${y - half}px;width:${2 * half}px;height:${2 * half}px`;
</script>

<div class="deck" bind:clientWidth={width} style:max-width="{Math.round(380 * most)}px" style:height="{Math.round(H * scale)}px" {@attach run}>
  <div class="stage" style:transform="translate(-50%, -50%) scale({scale})">
    <svg class="still" viewBox="0 0 {W} {H}" width={W} height={H} aria-hidden="true">
      <path class="tape" bind:this={tape} />
      {#each [REEL_L, REEL_R] as c, i (i)}
        <g transform="translate({c[0]} {c[1]})">
          <circle class="rim" r={MAXR + 1} />
          {#if i === 0}<circle class="pack" bind:this={packL} r={MAXR} />{:else}<circle class="pack" bind:this={packR} r={MAXR} />{/if}
          <circle class="hub" r={HUB} />
        </g>
      {/each}
      {#each WHEELS as w, i (i)}
        <circle class="wheel" cx={w.c[0]} cy={w.c[1]} r={w.r} />
      {/each}
      <path class="hill" d={hill(-1)} />
      <path class="hill" d={hill(1)} />
      {#if turnable}<text class="side" class:on={travel === 'L'} x={KNOB.x - 6.5} y={KNOB.y + 3.6} text-anchor="end">L</text>
      <text class="side" class:on={travel === 'R'} x={KNOB.x + 6.5} y={KNOB.y + 3.6} text-anchor="start">R</text>{/if}
    </svg>
    <!-- The direction knob (the owner, 2026-10-27): R forwards, L backwards; turned while playing, the tape slows to a
         stop before it goes the other way (lib/localPlayer turnTape). -->
    {#if turnable}<button
      class="knob"
      role="switch"
      class:stuck
      disabled={stuck}
      aria-checked={tapeDir.want === 'L'}
      aria-label="Play backwards"
      style="left:{KNOB.x - 6}px;top:{KNOB.y - 6}px"
      onclick={() => turnTape(tapeDir.want === 'L' ? 'R' : 'L')}
      ><svg viewBox="-6 -6 12 12" aria-hidden="true"
        ><g class="knob-turn" style:transform="rotate({tapeDir.want === 'L' ? -SWEEP : SWEEP}deg)"
          ><circle class="knob-skirt" r={KNOB.r + 1.1} /><path class="knob-pointer" d="M-1.15 3.3 L1.15 3.3 L0.55 -4.6 L-0.55 -4.6 Z" /><circle
            class="knob-pointer"
            r="1.25"
          /><path class="knob-index" d="M0 -4.2 L0 -1.9" /></g
        ></svg
      ></button
    >{/if}
    {#each [REEL_L, REEL_R] as c, i (i)}
      {@const spokes = [0, 1, 2].map((k) => [Math.cos((k * 2 * Math.PI) / 3) * (HUB + 3), Math.sin((k * 2 * Math.PI) / 3) * (HUB + 3)])}
      {#if i === 0}
        <div class="turns" style={box(c[0], c[1], SPOKES)} bind:this={spokesL}>
          <svg viewBox="{-SPOKES} {-SPOKES} {2 * SPOKES} {2 * SPOKES}">{#each spokes as [x, y], k (k)}<line class="spoke" x2={x} y2={y} />{/each}</svg>
        </div>
      {:else}
        <div class="turns" style={box(c[0], c[1], SPOKES)} bind:this={spokesR}>
          <svg viewBox="{-SPOKES} {-SPOKES} {2 * SPOKES} {2 * SPOKES}">{#each spokes as [x, y], k (k)}<line class="spoke" x2={x} y2={y} />{/each}</svg>
        </div>
      {/if}
    {/each}
    {#each WHEELS as w, i (i)}
      <div class="turns" style={box(w.c[0], w.c[1], w.r)} bind:this={notches[i]}>
        <svg viewBox="{-w.r} {-w.r} {2 * w.r} {2 * w.r}"><line class="notch" x2={w.r - 0.6} /></svg>
      </div>
    {/each}
    <!-- The head lit: its hills in the accent over the unlit ones, as bright as the downbeat. -->
    <div class="lit" style="left:{HEAD_BOX.x}px;top:{HEAD_BOX.y}px;width:{HEAD_BOX.w}px;height:{HEAD_BOX.h}px" bind:this={lit}>
      <svg viewBox="{HEAD_BOX.x} {HEAD_BOX.y} {HEAD_BOX.w} {HEAD_BOX.h}">
        <path class="hill on" d={hill(-1)} />
        <path class="hill on" d={hill(1)} />
      </svg>
    </div>
  </div>
</div>

<style>
  .deck {
    position: relative;
    width: 100%;
    max-width: 380px;
    height: 43px;
    margin: 0 auto;
  }

  /* The drawing at its own size, centred, scaled down only in a narrower row (as the SVG's own fit did). */
  .stage {
    position: absolute;
    left: 50%;
    top: 50%;
    width: 320px;
    height: 43px;
  }

  .still {
    display: block;
  }

  .turns,
  .lit {
    pointer-events: none;
  }

  /* The knob: a small round with its mark, turned a quarter either way; its box larger than it, for a finger. */
  .knob {
    position: absolute;
    z-index: 1;
    width: 12px;
    height: 12px;
    padding: 0;
    border: 0;
    background: none;
    cursor: pointer;
  }

  .knob svg {
    display: block;
    width: 100%;
    height: 100%;
    overflow: visible;
  }

  .knob.stuck {
    opacity: 0.35;
    cursor: default;
  }

  .knob::before {
    position: absolute;
    inset: -8px -14px;
    content: '';
  }

  /* The chicken-head turns about its centre to the side it is at. */
  .knob-turn {
    transform-box: fill-box;
    transform-origin: center;
    transition: transform 320ms cubic-bezier(0.3, 0.8, 0.25, 1);
  }

  @media (prefers-reduced-motion: reduce) {
    .knob-turn {
      transition: none;
    }
  }

  /* A dark skirt, a bright pointer over it with its hub, a dark line along the pointer's tip. */
  .knob-skirt {
    fill: color-mix(in srgb, var(--text) 7%, var(--surface));
    stroke: color-mix(in srgb, var(--text) 22%, transparent);
    stroke-width: 0.35;
  }

  .knob-pointer {
    fill: color-mix(in srgb, var(--text) 82%, var(--surface));
  }

  .knob-index {
    fill: none;
    stroke: var(--surface);
    stroke-width: 0.45;
    stroke-linecap: round;
  }

  /* Two and a half times the first (the owner, 2026-10-27), clear of the knob either side. */
  .side {
    fill: var(--muted);
    font-size: 10px;
    font-weight: 600;
    opacity: 0.7;
  }

  .side.on {
    fill: var(--text);
    opacity: 1;
  }

  .turns,
  .lit {
    position: absolute;
  }

  .turns {
    will-change: transform;
  }

  .lit {
    opacity: 0;
    will-change: opacity;
  }

  .turns svg,
  .lit svg {
    display: block;
    width: 100%;
    height: 100%;
    overflow: visible;
  }

  .tape {
    fill: none;
    stroke: var(--text);
    stroke-opacity: 0.65;
    stroke-width: 1.2;
  }

  .rim {
    fill: none;
    stroke: var(--muted);
    stroke-opacity: 0.35;
  }

  /* The packs in the waveform's colour (the cover's, as its played part; the owner, 2026-10-27). */
  .pack {
    fill: var(--art-accent, var(--accent));
    fill-opacity: 0.22;
    stroke: var(--art-accent, var(--accent));
    stroke-opacity: 0.6;
  }

  .hub {
    fill: none;
    stroke: var(--text);
    stroke-width: 1.2;
  }

  .spoke {
    stroke: var(--text);
    stroke-width: 1.6;
    stroke-linecap: round;
  }

  .wheel {
    fill: var(--surface);
    stroke: var(--muted);
    stroke-width: 1.1;
  }

  .notch {
    stroke: var(--text);
    stroke-width: 1;
    stroke-linecap: round;
  }

  /* The head's two hills, lit in the waveform's colour on each bar's first beat: the lit pair's opacity is the light,
     the same mix as the colours' own (85% of the accent at full). */
  .hill {
    fill: var(--surface);
    stroke: var(--muted);
    stroke-width: 1.1;
    stroke-linejoin: round;
  }

  .hill.on {
    fill: color-mix(in srgb, var(--art-accent, var(--accent)) 85%, var(--surface));
    stroke: color-mix(in srgb, var(--art-accent, var(--accent)) 85%, var(--muted));
  }
</style>
