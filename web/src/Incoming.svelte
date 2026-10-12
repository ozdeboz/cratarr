<script lang="ts">
  import {
    ApiError,
    deleteIncoming,
    getIncoming,
    incomingCoverUrl,
    playIncoming,
    undoDelete,
    type IncomingAlbum,
    type IncomingSource,
  } from './lib/api';
  import Popover from './lib/Popover.svelte';
  import { longpress } from './lib/longpress';
  import { prefs } from './lib/prefs.svelte';
  import { live } from './lib/live.svelte';
  import Notice, { type NoticeData } from './lib/Notice.svelte';

  let sources = $state<IncomingSource[] | null>(null);
  let error = $state<string | null>(null);
  let noCover = $state<Record<string, boolean>>({});
  let filter = $state('');
  // What is playing (the shell polls it), so the album being listened to stands out.
  const playing = $derived(live.view?.now_playing ?? null);
  let notice = $state<NoticeData | null>(null);
  let starting = $state<string | null>(null);

  // The open menu of one album (Play, Delete album), anchored to it; `ask` = the in-place delete question.
  let menu = $state<{ source: string; a: IncomingAlbum; anchor: DOMRect; ask: boolean } | null>(null);
  let working = $state(false);

  async function reload() {
    try {
      sources = (await getIncoming(prefs.source || undefined, true)).sources;
    } catch {
      /* the regular refresh will try again */
    }
  }

  async function remove(source: string, a: IncomingAlbum) {
    if (working) return;
    working = true;
    try {
      const { undo, title } = await deleteIncoming(source, a.id);
      sources = (sources ?? []).map((s) => (s.id === source ? { ...s, albums: s.albums.filter((x) => x.id !== a.id) } : s));
      notice = { text: `Deleted ${title}.`, undo: () => restore(undo) };
    } catch (e) {
      notice = { text: e instanceof ApiError ? e.message : 'Could not delete the album', tone: 'error' };
    } finally {
      working = false;
      menu = null;
    }
  }

  async function restore(undo: string) {
    try {
      const { restored } = await undoDelete(undo);
      notice = { text: `Restored ${restored}.` };
      await reload();
    } catch (e) {
      notice = { text: e instanceof ApiError ? e.message : 'Could not restore the album', tone: 'error' };
    }
  }

  async function play(source: string, a: IncomingAlbum) {
    if (starting) return;
    starting = a.id;
    try {
      const { player } = await playIncoming(source, a.id);
      notice = { text: `Playing ${a.title} on ${player}.` };
    } catch (e) {
      notice = { text: e instanceof ApiError ? e.message : 'Could not start playing', tone: 'error' };
    } finally {
      starting = null;
    }
  }

  // The server follows the source every 30 s and keeps the list. First show whatever copy answers fastest (the
  // service worker's), then ask for the latest, and keep asking every 30 s.
  $effect(() => {
    const want = prefs.source || undefined;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    async function load(fresh: boolean) {
      try {
        sources = (await getIncoming(want, fresh)).sources;
        error = null;
      } catch (e) {
        if (!sources) error = e instanceof ApiError ? e.message : 'Unexpected error';
      }
    }
    async function follow() {
      await load(true);
      if (!stopped) timer = setTimeout(follow, 30_000);
    }
    load(false).then(follow);
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  });

  const lossless = (a: IncomingAlbum) => a.format === 'FLAC';
  const quality = (a: IncomingAlbum) =>
    [a.format ?? 'Mixed formats', lossless(a) || a.bitrate_kbps === null ? null : `${a.bitrate_kbps} kbps`].filter(Boolean).join(' ');
  const added = (t: number | null) =>
    t === null ? '' : new Date(t * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
  const matches = (a: IncomingAlbum) => {
    const q = filter.trim().toLowerCase();
    return !q || `${a.artist ?? ''} ${a.title}`.toLowerCase().includes(q);
  };
</script>

<main>

  <Notice bind:notice />

  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if sources === null}
    <p class="empty">Loading…</p>
  {:else if sources.length === 0}
    <p class="empty">No server with an Incoming library is connected.</p>
  {:else}
    <input class="filter" type="search" placeholder="Filter albums" bind:value={filter} aria-label="Filter albums" />
    {#each sources as s (s.id)}
      {#if sources.length > 1}<h2>{s.label}</h2>{/if}
      {#if !s.supported}
        <p class="empty">{s.label} can't list albums.</p>
      {:else if s.error}
        <p class="error" role="alert">{s.error}</p>
      {:else if s.albums.length === 0}
        <p class="empty">Nothing incoming.</p>
      {:else}
        <p class="count">{s.albums.length} {s.albums.length === 1 ? 'album' : 'albums'}, newest first. Tap one to play it.</p>
        <ul>
          {#each s.albums.filter(matches) as a (a.id)}
            <li class:playing={playing?.album_id === a.id}>
              <button
                class="row"
                disabled={starting !== null}
                aria-busy={starting === a.id}
                onclick={() => play(s.id, a)}
                use:longpress={(anchor) => (menu = { source: s.id, a, anchor, ask: false })}
              >
              {#if noCover[a.id]}
                <div class="cover none" aria-hidden="true"></div>
              {:else}
                <img
                  class="cover"
                  src={incomingCoverUrl(s.id, a.id)}
                  alt=""
                  loading="lazy"
                  decoding="async"
                  onerror={() => (noCover[a.id] = true)}
                />
              {/if}
              <div class="text">
                <div class="title">{a.title}{#if a.year}<span class="year">{` (${a.year})`}</span>{/if}</div>
                <div class="artist">{a.artist ?? 'Unknown artist'}{#if playing?.album_id === a.id}<span class="now"> · playing</span>{/if}</div>
                <div class="meta">
                  <span>{a.tracks} {a.tracks === 1 ? 'track' : 'tracks'}</span>
                  <span class:weak={!lossless(a) && a.bitrate_kbps !== null && a.bitrate_kbps < 320}>{quality(a)}</span>
                  {#if a.added}<span>added {added(a.added)}</span>{/if}
                  {#each a.tags as t (t)}<span class="tag">{t}</span>{/each}
                </div>
                {#if s.verdicts?.[a.id]}
                  {@const v = s.verdicts[a.id]}
                  <div class="verdict" class:upgrade={v.kind === 'upgrade'} title={v.library}>
                    <span class="badge">{v.kind === 'upgrade' ? 'Upgrade' : 'In Music'}</span>
                    {v.why}
                  </div>
                {/if}
              </div>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    {/each}
  {/if}
</main>

{#if menu}
  {@const m = menu}
  <Popover anchor={m.anchor} label={m.a.title} onclose={() => (menu = null)}>
    <div class="head">{m.a.title} <span>{m.a.artist ?? 'Unknown artist'}</span></div>
    {#if !m.ask}
      <div class="items" role="menu">
        <button role="menuitem" onclick={() => { menu = null; play(m.source, m.a); }}>Play</button>
        <hr />
        <button role="menuitem" class="danger" onclick={() => menu && (menu.ask = true)}>Delete album</button>
      </div>
    {:else}
      <div class="form">
        <p class="ask">Delete {m.a.title}? It stays in the trash for 14 days, and you can undo this right after.</p>
        <div class="actions">
          <button class="danger solid" disabled={working} onclick={() => remove(m.source, m.a)}>Delete album</button>
          <button onclick={() => (menu = null)}>Cancel</button>
        </div>
      </div>
    {/if}
  </Popover>
{/if}

<style>
  main {
    max-width: 900px;
    padding: 0 clamp(12px, 2vw, 32px) 48px;
  }

  h2 {
    margin: 24px 0 6px;
    font-size: 1rem;
  }

  .empty,
  .count {
    color: var(--muted);
    font-size: 0.9rem;
  }

  .error {
    color: var(--danger);
  }

  .filter {
    width: 100%;
    max-width: 360px;
    min-height: 40px;
    margin: 8px 0 4px;
    padding: 0 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    color: var(--text);
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    border-bottom: 1px solid var(--border);
  }

  .row {
    display: flex;
    gap: 12px;
    align-items: center;
    width: 100%;
    padding: 10px 0;
    background: none;
    border: 0;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .row:hover .title {
    text-decoration: underline;
  }

  .row:disabled {
    cursor: progress;
  }

  .row[aria-busy='true'] {
    opacity: 0.6;
  }

  .cover {
    flex: none;
    width: 56px;
    height: 56px;
    border-radius: 8px;
    object-fit: cover;
    background: var(--surface);
  }

  .cover.none {
    border: 1px solid var(--border);
  }

  .text {
    min-width: 0;
  }

  .title {
    font-weight: 600;
    overflow-wrap: anywhere;
  }

  .year,
  .artist {
    color: var(--muted);
    font-weight: 400;
  }

  .artist {
    font-size: 0.9rem;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    margin-top: 2px;
    color: var(--muted);
    font-size: 0.8rem;
    font-variant-numeric: tabular-nums;
  }

  li.playing {
    margin: 0 -8px;
    padding-inline: 8px;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    border-radius: 10px;
  }

  .now {
    color: var(--accent);
    font-weight: 600;
  }

  .weak {
    color: var(--danger);
  }

  .tag {
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 999px;
  }

  .verdict {
    margin-top: 3px;
    color: var(--muted);
    font-size: 0.8rem;
  }

  .verdict .badge {
    margin-right: 6px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 999px;
    color: var(--text);
    font-weight: 600;
  }

  .verdict.upgrade .badge {
    border-color: var(--accent);
    color: var(--accent);
  }

  /* Album menu (same look as the playlist menu) */
  .head {
    padding: 8px 10px 6px;
    font-weight: 650;
    overflow-wrap: anywhere;
  }

  .head span {
    display: block;
    color: var(--muted);
    font-size: 0.8rem;
    font-weight: 400;
  }

  .items {
    display: grid;
  }

  .items button {
    min-height: 44px;
    padding: 0 10px;
    text-align: left;
    background: none;
    border: 0;
    border-radius: 8px;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }

  .items hr {
    margin: 4px 6px;
    border: 0;
    border-top: 1px solid var(--border);
  }

  .items button:hover,
  .items button:focus-visible {
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  .danger,
  .items button.danger {
    color: var(--danger);
  }

  .form {
    display: grid;
    gap: 10px;
    padding: 6px;
  }

  .ask {
    margin: 0;
    font-size: 0.92rem;
  }

  .actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .actions button {
    min-height: 40px;
    padding: 0 14px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }

  .actions button.danger.solid {
    background: var(--danger);
    border-color: var(--danger);
    color: #fff;
    font-weight: 600;
  }

  .actions button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
