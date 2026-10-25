<script lang="ts">
  // A thin bar along the top while something the user asked for is loading (lib/progress.svelte.ts). It shows only
  // after a moment, so quick answers never flash it; creeps towards the end while it waits (how long is unknown);
  // and once shown, finishes its run before fading, so it never blinks.
  import { untrack } from 'svelte';
  import { loading } from './progress.svelte';

  /** Answers quicker than this show nothing. */
  const DELAY_MS = 300;
  /** Once shown, at least this long. */
  const MIN_MS = 400;

  let shown = $state(false);
  let done = $state(false);
  let value = $state(0);
  let since = 0;
  let timers: ReturnType<typeof setTimeout>[] = [];
  const clear = () => {
    timers.forEach(clearTimeout);
    timers = [];
  };

  $effect(() => {
    const busy = loading.count > 0;
    clear();
    if (busy && !shown) {
      timers.push(
        setTimeout(() => {
          shown = true;
          done = false;
          value = 0.15;
          since = performance.now();
        }, DELAY_MS),
      );
    } else if (!busy && shown) {
      timers.push(
        setTimeout(
          () => {
            value = 1;
            done = true;
            timers.push(setTimeout(() => ((shown = false), (value = 0)), 300));
          },
          Math.max(0, MIN_MS - (performance.now() - since)),
        ),
      );
    } else if (busy && shown && untrack(() => done)) {
      // Something new while it was finishing: run again.
      done = false;
      value = 0.15;
    }
    return clear;
  });

  // Work that says how far it is: the bar there (never back), else it creeps.
  $effect(() => {
    const p = loading.part;
    if (p !== null && shown && !done) value = Math.max(untrack(() => value), 0.05 + 0.9 * p);
  });
  // While it waits, creep: a little less each step, never reaching the end.
  $effect(() => {
    if (!shown || done || loading.part !== null) return;
    const t = setInterval(() => (value += (0.9 - value) * 0.08), 250);
    return () => clearInterval(t);
  });
</script>

{#if shown}
  <div class="bar" class:done role="progressbar" aria-label="Loading" aria-valuetext="Loading">
    <div class="fill" style:transform="scaleX({value})"></div>
  </div>
{/if}

<style>
  .bar {
    position: fixed;
    inset: 0 0 auto;
    z-index: 40;
    height: 3px;
    pointer-events: none;
    transition: opacity 300ms ease;
  }

  .bar.done {
    opacity: 0;
    transition-delay: 150ms;
  }

  .fill {
    height: 100%;
    background: var(--accent);
    box-shadow: 0 0 8px var(--accent);
    transform-origin: left;
    transition: transform 250ms ease-out;
  }

  @media (prefers-reduced-motion: reduce) {
    .fill,
    .bar {
      transition: none;
    }
  }

  /* Animations off on this device (Display): the same as the system's reduced motion. */
  :global(:root[data-motion='off']) .fill,
  :global(:root[data-motion='off']) .bar {
    transition: none;
  }
</style>
