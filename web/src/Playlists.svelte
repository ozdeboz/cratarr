<script lang="ts">
  import { ApiError, getState, toggle, type Playlist, type State } from './lib/api';
  import { remember } from './lib/sources.svelte';
  import { prefs } from './lib/prefs.svelte';
  import NowPlaying from './lib/NowPlaying.svelte';

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

  // Restarts (and polls at once) when the followed source changes.
  $effect(() => {
    const followed = prefs.source || undefined;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        view = await getState(followed);
        remember(view.sources);
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
      view = await toggle(p.id, prefs.source || undefined);
    } catch (e) {
      notice = e instanceof ApiError ? e.message : 'Something went wrong';
    } finally {
      delete optimistic[p.id];
      delete busy[p.id];
    }
  }
</script>

<main>
  <NowPlaying {np}>
    {#snippet below()}
      {#if np && !np.addable}<span class="badge">Not in the library: cannot be added to playlists</span>{/if}
    {/snippet}
    {#snippet aside()}
      <div class="count" aria-live="polite">
        <strong>{memberCount}</strong>
        <span>{memberCount === 1 ? 'playlist' : 'playlists'}</span>
      </div>
    {/snippet}
  </NowPlaying>

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
