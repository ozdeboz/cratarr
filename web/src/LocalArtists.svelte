<script lang="ts">
  // Local mode's Artists (decision 0052): the device's own music by its tracks' artists, A to Z, as rows (how many
  // albums and tracks); the page's search filters them. A tap views the artist in the panel (their tracks, album by
  // album).
  import { panelBeside } from './lib/glass.svelte';
  import { askLibrary, device, DEVICE, deviceArtists } from './lib/deviceMusic.svelte';
  import { plural } from './lib/insights.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import { artistSubject, panel, viewPlaylist } from './lib/panel.svelte';
  import { matchesSearch, pageView } from './lib/pageView.svelte';
  import Row from './lib/Row.svelte';

  $effect(() => {
    askLibrary();
  });
  // As every search reads (lib/search, decision 0072).
  const shown = $derived(deviceArtists().filter((a) => matchesSearch(a.name)));
</script>

<main>
  {#if device.denied}
    <p class="empty">Allow the app to read this device's music in Settings → This device.</p>
  {:else if !device.tracks}
    <PageLoading />
  {:else if !shown.length}
    <p class="empty">No artists.</p>
  {:else}
    <ol class="rows">
      {#each shown as a (a.name)}
        <li>
          <Row
            size="medium"
            title={a.name}
            sub={`${plural(a.albums, 'album')} · ${plural(a.tracks, 'track')}`}
            active={panelBeside() && panel.shown === artistSubject(DEVICE, a.name)}
            onclick={() => viewPlaylist(artistSubject(DEVICE, a.name))}
          />
        </li>
      {/each}
    </ol>
  {/if}
</main>

<style>
  main {
    max-width: 900px;
    padding: 6px clamp(12px, 2vw, 32px) 48px;
  }

  .rows {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .empty {
    color: var(--muted);
    font-size: 0.9rem;
  }
</style>
