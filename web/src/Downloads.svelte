<script lang="ts">
  // Downloads (decision 0033, step 4; 0038 amended 2026-10-24; laid out as Requests, Insights, Health and Settings, the
  // owner 2026-10-24: the page an index, its lists in its own panel, never the content panel): This device, Synced and
  // Downloaded once, each its icon, its name, how many in a line, and ›. A tap opens the list in the panel
  // (lib/DownloadsPanel): beside the index on a roomy window, where one is always open (App), its row tinted; over it
  // on a phone. The pill: its search narrows the lists, and its button syncs now (two steps, a toast at the end).
  import { panelBeside } from './lib/glass.svelte';
  import { askDevice, askDownloads, downloads, inApp } from './lib/app.svelte';
  import { library, readLibrary } from './lib/albums.svelte';
  import { DOWNLOADS_LISTS, onceAlbums, WAITS, type DownloadsList } from './lib/downloadsLists.svelte';
  import Icon from './lib/Icon.svelte';
  import { askKept, kept, keptBusy, syncKept } from './lib/kept.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import { DOWNLOADS, downloadsOf, panel, viewPlaylist } from './lib/panel.svelte';
  import { usePageAction } from './lib/pageView.svelte';
  import { downloadedSize } from './lib/settings/editor.svelte';
  import BrowserDownloads from './lib/BrowserDownloads.svelte';

  // The app's; the installed page's own are in this browser (lib/offline).
  if (inApp) {
    askDownloads();
    askKept();
    askDevice();
  }
  // The Library's list, for what each album is (the kept copy first; without a connection that is all there is).
  if (!library.view) readLibrary(false).catch(() => {});

  const count = (n: number, one: string, many: string) => `${n.toLocaleString()} ${n === 1 ? one : many}`;
  /** A list's line: how many, and how it stands. */
  function line(id: DownloadsList): string {
    if (id === 'device') {
      const used = kept.known ? kept.used : downloads.bytes;
      const failed = downloads.tracks.filter((t) => t.download.state === 'failed').length;
      return [
        count(downloads.tracks.length, 'track', 'tracks'),
        kept.budget ? `${downloadedSize(used)} of ${downloadedSize(kept.budget)}` : downloadedSize(used),
        kept.waiting ? WAITS[kept.waiting] : null,
        failed ? `${failed} failed` : null,
      ]
        .filter(Boolean)
        .join(' · ');
    }
    if (id === 'synced') return kept.items.length ? count(kept.items.length, 'item', 'items') + (keptBusy() ? ' · syncing' : '') : 'Nothing synced';
    const n = onceAlbums().length;
    return n ? count(n, 'album', 'albums') : 'Nothing downloaded once';
  }

  // The pill's button: sync now, ringed while anything syncs or waits.
  usePageAction(() => {
    if (!inApp || !kept.items.length) return null;
    const want = kept.items.reduce((n, i) => n + i.total - i.over_budget, 0);
    const done = kept.items.reduce((n, i) => n + i.done, 0);
    const busy = keptBusy();
    return { icon: 'refresh', label: 'Sync now', run: syncKept, progress: busy ? (want ? done / want : 0) : null, line: busy ? `Syncing · ${done} of ${want}` : null };
  });
</script>

<main>
  <!-- The installed page's own (0.9.8, lib/BrowserDownloads); the app's lists below. -->
  {#if !inApp}
    <BrowserDownloads />
  {:else if !downloads.known}
    <PageLoading />
  {:else}
    <section class="group" aria-label="Downloads">
      <div class="list">
        {#each DOWNLOADS_LISTS as l (l.id)}
          {@const open = panelBeside() && downloadsOf(panel.viewed) === l.id}
          <button class="row" class:open aria-pressed={open} onclick={() => viewPlaylist(DOWNLOADS + l.id)}>
            <Icon name={l.icon} size={24} />
            <span class="text">
              <span class="name">{l.label}</span>
              <small>{line(l.id)}</small>
            </span>
            <span class="chev"><Icon name="chevronRight" size={20} /></span>
          </button>
        {/each}
      </div>
    </section>
  {/if}
</main>

<style>
  /* As Insights' and Requests' index: one column, beside a list it is the index's own column (App). */
  main {
    padding: 16px clamp(12px, 2vw, 32px) 96px;
  }

  .group {
    max-width: 640px;
  }

  .list {
    overflow: hidden;
    border-radius: 14px;
    background: color-mix(in srgb, var(--text) 6%, transparent);
  }

  .row {
    display: flex;
    align-items: center;
    gap: 16px;
    width: 100%;
    min-height: 64px;
    padding: 10px 14px 10px 16px;
    border: 0;
    border-radius: 0;
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .row + .row {
    border-top: 1px solid var(--border);
  }

  .row:hover,
  .row:focus-visible {
    background: color-mix(in srgb, var(--text) 5%, transparent);
  }

  /* The list open beside the index. */
  .row.open {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }

  .row > :global(svg) {
    flex: none;
    color: var(--accent);
  }

  .text {
    display: grid;
    flex: 1;
    gap: 2px;
    min-width: 0;
  }

  .name {
    font-size: 15px;
  }

  .text small {
    overflow: hidden;
    color: var(--muted);
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chev {
    display: grid;
    flex: none;
    color: var(--muted);
  }
</style>
