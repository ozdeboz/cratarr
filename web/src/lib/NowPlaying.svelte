<script lang="ts">
  // The now-playing header, shared by the pages that show what is playing. The page polls the state and passes the
  // track in; this component owns the look and the progress bar. Page-specific bits go in the `aside` (right) and
  // `below` (under the progress bar) snippets.
  import { untrack, type Snippet } from 'svelte';
  import { coverUrl, type NowPlaying } from './api';
  import { prefs } from './prefs.svelte';

  let {
    np,
    cover = undefined,
    aside,
    below,
  }: {
    np: NowPlaying | null;
    /** Cover image URL; defaults to the library cover of the playing track. `null` = no cover. */
    cover?: string | null;
    aside?: Snippet;
    below?: Snippet;
  } = $props();

  const src = $derived(cover === undefined ? (np ? coverUrl(np) : null) : cover);
  // Covers that failed to load (not in the library, or none there): no image, no gap.
  let failed = $state<Record<string, boolean>>({});

  // The bar runs on a local clock between polls. It is re-anchored to the server only when the server disagrees
  // with it (track, pause state, or more than 1.5 s off: a seek or drift), so it moves smoothly.
  let anchor = $state<{ id: string; pos: number; at: number; paused: boolean } | null>(null);
  let tick = $state(performance.now());

  $effect(() => {
    const p = np?.progress;
    if (!np || !p) {
      anchor = null;
      return;
    }
    const now = performance.now();
    const a = untrack(() => anchor);
    const predicted = a ? a.pos + (a.paused ? 0 : (now - a.at) / 1000) : 0;
    if (!a || a.id !== np.id || a.paused !== p.paused || Math.abs(predicted - p.position_secs) > 1.5) {
      anchor = { id: np.id, pos: p.position_secs, at: now, paused: p.paused };
    }
  });

  $effect(() => {
    if (!anchor || anchor.paused) return;
    const t = setInterval(() => {
      if (!document.hidden) tick = performance.now();
    }, 250);
    return () => clearInterval(t);
  });

  const duration = $derived(np?.progress?.duration_secs ?? null);
  const position = $derived.by(() => {
    if (!anchor) return 0;
    const p = anchor.paused ? anchor.pos : anchor.pos + Math.max(0, tick - anchor.at) / 1000;
    return duration ? Math.min(p, duration) : p;
  });

  function clock(secs: number) {
    const s = Math.floor(secs);
    const mm = String(Math.floor((s % 3600) / 60));
    const ss = String(s % 60).padStart(2, '0');
    return s >= 3600 ? `${Math.floor(s / 3600)}:${mm.padStart(2, '0')}:${ss}` : `${mm}:${ss}`;
  }
</script>

<header>
  {#if np && src && !failed[src]}
    {#key src}
      <img class="cover" {src} alt="" decoding="async" onerror={() => (failed[src] = true)} />
    {/key}
  {/if}
  <div class="np">
    {#if np}
      <div class="title">{np.name}</div>
      <div class="sub">{[np.artist, np.album].filter(Boolean).join(' · ')}</div>
      {#if np.progress && prefs.showProgress}
        <div
          class="progress"
          class:paused={np.progress.paused}
          class:estimated={np.progress.estimated}
          title={np.progress.estimated
            ? 'Estimated: no player reports this track, so this counts from when it started'
            : np.progress.paused
              ? 'Paused'
              : undefined}
        >
          <span class="t">{np.progress.estimated ? '≈' : ''}{clock(position)}</span>
          <div
            class="bar"
            role="progressbar"
            aria-label={np.progress.paused ? 'Track position, paused' : 'Track position'}
            aria-valuemin={0}
            aria-valuemax={duration ?? undefined}
            aria-valuenow={Math.floor(position)}
            aria-valuetext={duration ? `${clock(position)} of ${clock(duration)}` : clock(position)}
          >
            <div class="fill" style:transform="scaleX({duration ? position / duration : 0})"></div>
          </div>
          <span class="t">{duration ? clock(duration) : '–:––'}</span>
        </div>
      {/if}
      {@render below?.()}
    {:else}
      <div class="title idle">Nothing is playing</div>
    {/if}
  </div>
  {#if np}{@render aside?.()}{/if}
</header>

<style>
  header {
    position: sticky;
    top: 0;
    z-index: 2;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 16px 0 12px;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
  }

  /* A background layer reaching into the page's side padding (the pages' own clamp), so a row that bleeds wider
     (the playing album on Incoming) never shows through while it scrolls under the header. Not further: on
     desktop the sidebar sits right next to that padding. (A clipped box-shadow did the same but made rendering
     hang with many images below, observed in Chrome on 0.2.5.) */
  header::before {
    content: '';
    position: absolute;
    inset: 0 calc(-1 * clamp(12px, 2vw, 32px));
    z-index: -1;
    background: var(--bg);
  }


  .np {
    flex: 1;
    min-width: 0;
  }

  .progress {
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: 640px;
    margin-top: 8px;
    font-size: 0.8rem;
    font-variant-numeric: tabular-nums;
    color: var(--muted);
  }

  .progress .t {
    flex: none;
    min-width: 3.2em;
  }

  .progress .t:last-child {
    text-align: right;
  }

  .bar {
    flex: 1;
    height: 4px;
    border-radius: 2px;
    background: var(--border);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    background: var(--accent);
    transform-origin: left;
    transition: transform 250ms linear;
  }

  .progress.paused .fill {
    background: var(--muted);
  }

  .progress.estimated .fill {
    opacity: 0.5;
  }

  .cover {
    flex: none;
    width: 56px;
    height: 56px;
    border-radius: 8px;
    object-fit: cover;
    background: var(--surface);
  }

  .title {
    font-size: 1.25rem;
    font-weight: 650;
    line-height: 1.25;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .title.idle {
    color: var(--muted);
    font-weight: 500;
  }

  .sub {
    margin-top: 2px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
