<script lang="ts">
  import { untrack } from 'svelte';
  import { ApiError, getState, toggle, type Playlist, type State } from './lib/api';
  import { prefs } from './lib/prefs.svelte';

  let view = $state<State | null>(null);
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let filter = $state('');
  // Zoom = chip width, which sets how many columns fit; one width per step in prefs.ZOOM_STEPS.
  const WIDTHS = [110, 140, 170, 210, 260, 340];
  const setZoom = (next: number) => (prefs.zoom = Math.min(WIDTHS.length - 1, Math.max(0, next)));
  // Taps flip the chip at once; the server's answer replaces this guess.
  let optimistic = $state<Record<string, boolean>>({});
  let busy = $state<Record<string, boolean>>({});

  $effect(() => {
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        view = await getState();
        error = null;
      } catch (e) {
        error = e instanceof ApiError ? e.message : 'Unexpected error';
      }
      if (!stopped) timer = setTimeout(poll, document.hidden ? 5000 : 2000);
    }
    poll();
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  });

  $effect(() => {
    if (!notice) return;
    const t = setTimeout(() => (notice = null), 6000);
    return () => clearTimeout(t);
  });

  const np = $derived(view?.now_playing ?? null);
  // The bar runs on a local clock between 2-second polls. It is re-anchored to the server only when the server
  // disagrees with it (track, pause state, or more than 1.5 s off: a seek or drift), so it moves smoothly.
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

  // Track ids whose cover failed to load (not in the library, or no cover file): no image, no gap.
  let noCover = $state<Record<string, boolean>>({});

  const items = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    let list = (view?.playlists ?? []).map((p) => {
      const member = optimistic[p.id] ?? p.member;
      return { ...p, member, count: p.count + (member === p.member ? 0 : member ? 1 : -1) };
    });
    if (q) list = list.filter((p) => p.name.toLowerCase().includes(q));
    if (prefs.membersFirst) list = [...list].sort((a, b) => Number(b.member) - Number(a.member));
    return list;
  });

  const memberCount = $derived(
    (view?.playlists ?? []).filter((p) => optimistic[p.id] ?? p.member).length,
  );

  async function tap(p: Playlist) {
    if (busy[p.id] || !np) return;
    if (!np.addable && !p.member) {
      notice = 'This track is not in the curated library, so it cannot be added to a playlist.';
      return;
    }
    busy[p.id] = true;
    optimistic[p.id] = !p.member;
    notice = null;
    try {
      view = await toggle(p.id);
    } catch (e) {
      notice = e instanceof ApiError ? e.message : 'Something went wrong';
    } finally {
      delete optimistic[p.id];
      delete busy[p.id];
    }
  }
</script>

<main>
  <header>
    {#if np && !noCover[np.id]}
      {#key np.id}
        <img class="cover" src="/api/cover/{np.id}" alt="" decoding="async" onerror={() => np && (noCover[np.id] = true)} />
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
        {#if !np.addable}<span class="badge">Not in the library: cannot be added to playlists</span>{/if}
      {:else}
        <div class="title idle">Nothing is playing</div>
      {/if}
    </div>
    {#if np}
      <div class="count" aria-live="polite">
        <strong>{memberCount}</strong>
        <span>{memberCount === 1 ? 'playlist' : 'playlists'}</span>
      </div>
    {/if}
  </header>

  <div class="tools">
    <input type="search" placeholder="Filter playlists" aria-label="Filter playlists" bind:value={filter} />
    <div class="seg" role="group" aria-label="Sort order">
      <button aria-pressed={!prefs.membersFirst} onclick={() => (prefs.membersFirst = false)}>A–Z</button>
      <button aria-pressed={prefs.membersFirst} onclick={() => (prefs.membersFirst = true)}>In first</button>
    </div>
    <div class="seg" role="group" aria-label="Zoom">
      <button aria-label="Zoom out, more columns" disabled={prefs.zoom === 0} onclick={() => setZoom(prefs.zoom - 1)}>−</button>
      <button aria-label="Zoom in, fewer columns" disabled={prefs.zoom === WIDTHS.length - 1} onclick={() => setZoom(prefs.zoom + 1)}>+</button>
    </div>
  </div>

  {#if error}<div class="banner error" role="alert">{error}</div>{/if}
  {#if notice}<div class="banner warn" role="status">{notice}</div>{/if}

  {#if !view && !error}
    <p class="empty">Loading…</p>
  {:else}
    <ul class="grid" style:--min="{WIDTHS[prefs.zoom]}px">
      {#each items as p (p.id)}
        <li>
          <button
            class="chip"
            class:member={p.member}
            class:busy={busy[p.id]}
            class:blocked={!!np && !np.addable && !p.member}
            aria-pressed={p.member}
            title={p.dead > 0 ? `${p.dead} ${p.dead === 1 ? 'entry points' : 'entries point'} at missing files` : undefined}
            disabled={!np}
            onclick={() => tap(p)}
          >
            <span class="mark" aria-hidden="true">{p.member ? '✓' : '+'}</span>
            <span class="name">{p.name}</span>
            <span class="n">{p.count}{#if p.dead > 0}<i class="dead" aria-hidden="true"></i>{/if}</span>
          </button>
        </li>
      {/each}
    </ul>
    {#if view && items.length === 0}<p class="empty">No playlists match.</p>{/if}
  {/if}
</main>

<style>
  main {
    padding: 0 clamp(12px, 2vw, 32px) 48px;
  }

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

  .badge {
    display: inline-block;
    margin-top: 8px;
    padding: 2px 10px;
    border: 1px solid var(--warn);
    border-radius: 999px;
    color: var(--warn);
    font-size: 0.8rem;
  }

  .count {
    flex: none;
    text-align: center;
    line-height: 1.1;
  }

  .count strong {
    display: block;
    font-size: 1.8rem;
    color: var(--accent);
  }

  .count span {
    color: var(--muted);
    font-size: 0.75rem;
  }

  .tools {
    display: flex;
    gap: 12px;
    margin: 14px 0;
    flex-wrap: wrap;
  }

  input[type='search'] {
    flex: 1 1 220px;
    min-height: 44px;
    padding: 0 14px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
  }

  .seg {
    display: flex;
    border: 1px solid var(--border);
    border-radius: 10px;
    overflow: hidden;
  }

  .seg button {
    min-height: 44px;
    padding: 0 16px;
    background: var(--surface);
    border: 0;
    cursor: pointer;
  }

  .seg button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .seg button[aria-pressed='true'] {
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  .banner {
    margin-bottom: 12px;
    padding: 10px 14px;
    border-radius: 10px;
    border: 1px solid var(--border);
    background: var(--surface);
    font-size: 0.9rem;
  }

  .banner.warn {
    border-color: var(--warn);
    color: var(--warn);
  }

  .banner.error {
    border-color: var(--danger);
    color: var(--danger);
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(var(--min, 170px), 1fr));
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .chip {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 56px;
    padding: 8px 14px;
    text-align: left;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    cursor: pointer;
    transition:
      background 120ms,
      border-color 120ms,
      transform 80ms;
  }

  .chip:active:not(:disabled) {
    transform: scale(0.97);
  }

  .chip.member {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  .chip:disabled {
    cursor: default;
    opacity: 0.45;
  }

  .chip.busy {
    opacity: 0.7;
  }

  /* Track outside the library: adding is refused, so the add chips look blocked (tapping explains why). */
  .chip.blocked {
    opacity: 0.5;
    border-style: dashed;
  }

  .mark {
    flex: none;
    width: 1.2em;
    text-align: center;
    font-weight: 700;
  }

  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }

  .n {
    flex: none;
    margin-left: auto;
    font-size: 0.8rem;
    font-variant-numeric: tabular-nums;
    opacity: 0.65;
  }

  .dead {
    display: inline-block;
    width: 6px;
    height: 6px;
    margin-left: 5px;
    border-radius: 50%;
    background: var(--warn);
    vertical-align: middle;
  }

  .empty {
    margin-top: 32px;
    text-align: center;
    color: var(--muted);
  }
</style>
