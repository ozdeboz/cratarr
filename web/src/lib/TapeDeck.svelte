<script lang="ts">
  // The tape deck in the player's row under the title, while an effect runs (lib/NowPlaying; the owner, 2026-10-27):
  // its reels and wheels turn at the playing speed (a Vinyl ramp's speed, a rewind's backwards), its packs follow the
  // place in the track (lib/tapeDeck), and its head lights on each bar's first beat (lib/visuals/downbeat). Drawn a
  // frame at a time, straight onto its elements.
  import { appInfo, inApp } from './app.svelte';
  import { rewindNow } from './localPlayer.svelte';
  import { isLocal } from './output.svelte';
  import { prefs } from './prefs.svelte';
  import { ramping } from './ramping.svelte';
  import { duration, playing, position, shownPaused } from './remote.svelte';
  import { hill, HUB, MAXR, packs, REEL_L, REEL_R, tapePath, WHEELS } from './tapeDeck';
  import { downbeatAt } from './visuals/downbeat';

  /** Radians a second a wheel of radius 1 turns at 1× (as the mock looked right). */
  const TURN = 2.4 * Math.PI * 12;
  /** The fastest the picture turns, whatever the tape does (past it the spokes would strobe). */
  const MOST = 16;

  let tape: SVGPathElement;
  let packL: SVGCircleElement;
  let packR: SVGCircleElement;
  let spokesL: SVGGElement;
  let spokesR: SVGGElement;
  let notches: SVGLineElement[] = [];
  let hills: SVGPathElement[] = [];

  /** The tape's speed now (× the track's; negative: backwards). */
  let speed = 0;
  let lastPos: number | null = null;
  let lastAt = 0;
  function speedNow(now: number, pos: number): number {
    const np = playing();
    const rewinding = isLocal() ? rewindNow.on : !!np?.progress?.rewinding;
    const mode = (inApp ? appInfo.startStop : prefs.startStop).mode;
    const dt = Math.max(0.001, (now - lastAt) / 1000);
    const moved = lastPos === null ? 0 : (pos - lastPos) / dt;
    lastPos = pos;
    lastAt = now;
    if (rewinding) return speed * 0.85 + moved * 0.15;
    if (ramping.dir) {
      if (mode !== 'vinyl') return 1;
      const u = Math.min(1, Math.max(0, (now - ramping.at) / Math.max(1, ramping.ms)));
      return ramping.dir === 'out' ? 1 - u * u : 1 - (1 - u) ** 2;
    }
    return shownPaused() ? 0 : 1;
  }

  const angles = { l: 0, r: 0, w: WHEELS.map(() => 0) };
  function draw(now: number, dt: number) {
    const np = playing();
    const pos = position();
    const dur = duration();
    speed = speedNow(now, pos);
    const v = Math.max(-MOST, Math.min(MOST, speed));
    const place = dur ? Math.min(0.98, Math.max(0.02, pos / dur)) : 0.5;
    const [rl, rr] = packs(place);
    packL.setAttribute('r', rl.toFixed(2));
    packR.setAttribute('r', rr.toFixed(2));
    tape.setAttribute('d', tapePath(place));
    angles.l += (v * TURN * dt) / rl;
    angles.r += (v * TURN * dt) / rr;
    spokesL.setAttribute('transform', `rotate(${(angles.l * 180) / Math.PI})`);
    spokesR.setAttribute('transform', `rotate(${(angles.r * 180) / Math.PI})`);
    WHEELS.forEach((w, i) => {
      angles.w[i] += (v * TURN * dt) / w.r;
      notches[i]?.setAttribute('transform', `rotate(${(angles.w[i] * 180) / Math.PI})`);
    });
    // The head: lit on the downbeat, in the accent.
    const lit = np ? downbeatAt(np.source, np.id, pos) : 0;
    for (const h of hills) h.style.setProperty('--lit', `${Math.round(lit * 85)}%`);
  }

  function run(_: SVGSVGElement) {
    let frame = 0;
    let last = performance.now();
    const tick = (now: number) => {
      draw(now, Math.min(50, now - last) / 1000);
      last = now;
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }
</script>

<svg class="deck" viewBox="0 0 320 43" aria-hidden="true" {@attach run}>
  <path class="tape" bind:this={tape} />
  {#each [REEL_L, REEL_R] as c, i (i)}
    <g transform="translate({c[0]} {c[1]})">
      <circle class="rim" r={MAXR + 1} />
      {#if i === 0}<circle class="pack" bind:this={packL} r={MAXR} />{:else}<circle class="pack" bind:this={packR} r={MAXR} />{/if}
      <circle class="hub" r={HUB} />
      {#if i === 0}
        <g bind:this={spokesL}>{#each [0, 1, 2] as k (k)}<line class="spoke" x2={Math.cos((k * 2 * Math.PI) / 3) * (HUB + 3)} y2={Math.sin((k * 2 * Math.PI) / 3) * (HUB + 3)} />{/each}</g>
      {:else}
        <g bind:this={spokesR}>{#each [0, 1, 2] as k (k)}<line class="spoke" x2={Math.cos((k * 2 * Math.PI) / 3) * (HUB + 3)} y2={Math.sin((k * 2 * Math.PI) / 3) * (HUB + 3)} />{/each}</g>
      {/if}
    </g>
  {/each}
  {#each WHEELS as w, i (i)}
    <g transform="translate({w.c[0]} {w.c[1]})">
      <circle class="wheel" r={w.r} />
      <line class="notch" bind:this={notches[i]} x2={w.r - 0.6} />
    </g>
  {/each}
  <path class="hill" bind:this={hills[0]} d={hill(-1)} />
  <path class="hill" bind:this={hills[1]} d={hill(1)} />
</svg>

<style>
  .deck {
    display: block;
    width: 100%;
    max-width: 380px;
    height: 43px;
    margin: 0 auto;
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

  .pack {
    fill: var(--accent);
    fill-opacity: 0.22;
    stroke: var(--accent);
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

  /* The head's two hills, lit in the accent on each bar's first beat. */
  .hill {
    --lit: 0%;
    fill: color-mix(in srgb, var(--accent) var(--lit), var(--surface));
    stroke: color-mix(in srgb, var(--accent) var(--lit), var(--muted));
    stroke-width: 1.1;
    stroke-linejoin: round;
  }
</style>
