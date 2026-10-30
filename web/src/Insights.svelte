<script lang="ts">
  // Insights (decision 0065; laid out as Settings and Health, the owner 2026-10-24): its views as rows in groups
  // (Collection, Listening, Playlists), each its icon, its name, a number in a line, and ›. A tap opens the view in the
  // panel (lib/InsightsPanel): beside the list on a roomy window, where one is always open (App: the one last open), its
  // row tinted; over the list on a phone.
  import { panelBeside } from './lib/glass.svelte';
  import Icon from './lib/Icon.svelte';
  import { insights } from './lib/insights.svelte';
  import { INSIGHTS_GROUPS, INSIGHTS_VIEWS, type InsightsView } from './lib/insightsViews';
  import { INSIGHTS, insightsOf, panel, viewPlaylist } from './lib/panel.svelte';
  import { matchesSearch } from './lib/pageView.svelte';
  import { getDiscover } from './lib/api';

  $effect(() => insights.load());
  // The pill's search (the owner, 2026-10-25, as Settings'): a view whose name or group holds every word; the lists in
  // the panel narrow by it too.
  const groups = $derived(
    INSIGHTS_GROUPS.map((g) => ({ title: g, views: INSIGHTS_VIEWS.filter((v) => v.group === g && !v.under && matchesSearch(v.label, v.group)) })).filter((g) => g.views.length),
  );

  const count = (n: number, one: string, many: string) => `${n.toLocaleString()} ${n === 1 ? one : many}`;
  // Discovery's number (the owner, 2026-10-25): Discover's suggestions, read once with the page.
  let suggestions = $state<number | null>(null);
  $effect(() => {
    getDiscover().then(
      (v) => (suggestions = v.found?.albums.length ?? 0),
      () => {},
    );
  });
  /** A view's number, where the collection's answer has one. */
  function line(view: InsightsView): string | null {
    const d = insights.data;
    if (!d) return null;
    switch (view) {
      case 'overview':
        return `${count(d.library.tracks, 'track', 'tracks')} · ${count(d.library.albums, 'album', 'albums')}`;
      case 'genres':
        return count(d.library.genres.length, 'genre', 'genres');
      case 'artists':
        return count(d.library.artists, 'artist', 'artists');
      case 'tracks':
        return `${d.playlists.library_tracks_in_playlists.toLocaleString()} in a playlist`;
      case 'map':
        return count(d.playlists.playlists, 'playlist', 'playlists');
      case 'discover':
        return suggestions === null ? null : count(suggestions, 'suggestion', 'suggestions');
      default:
        return null;
    }
  }
</script>

<main>
  {#each groups as group (group.title)}
    <section class="group" aria-labelledby="insights-{group.title}">
      <h2 id="insights-{group.title}">{group.title}</h2>
      <div class="list">
        {#each group.views as v (v.id)}
          {@const open = panelBeside() && insightsOf(panel.viewed) === v.id}
          {@const l = line(v.id)}
          <button class="row" class:open aria-pressed={open} onclick={() => viewPlaylist(INSIGHTS + v.id)}>
            <Icon name={v.icon} size={24} />
            <span class="text">
              <span class="name">{v.label}</span>
              {#if l}<small>{l}</small>{/if}
            </span>
            <span class="chev"><Icon name="chevronRight" size={20} /></span>
          </button>
        {/each}
      </div>
    </section>
  {/each}
</main>

<style>
  /* As Settings' list (Settings.svelte): one column, beside a view it is the list's own column (App). */
  main {
    padding: 16px clamp(12px, 2vw, 32px) 96px;
  }

  .group {
    max-width: 640px;
  }

  .group + .group {
    margin-top: 24px;
  }

  h2 {
    margin: 0 4px 8px;
    color: var(--muted);
    font-size: 13px;
    font-weight: 600;
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

  /* The view open beside the list. */
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
