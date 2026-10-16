<script lang="ts">
  // A menu's Add to playlist steps (an album's, an artist's; lib/tracksToPlaylist): the playlists as toggle cards, each
  // ticked when it holds them all, and when one holds some already the question: add them all again, or only the others.
  import MenuEntry from './MenuEntry.svelte';
  import PlaylistToggles from './PlaylistToggles.svelte';
  import type { ToPlaylistStep, TracksToPlaylist } from './tracksToPlaylist.svelte';

  let { to, step = $bindable(), onclose }: { to: TracksToPlaylist; step: ToPlaylistStep; onclose: () => void } = $props();

  $effect(() => {
    if (step === 'lists') to.ask(to.ids());
  });
  const go = (s: ToPlaylistStep) => (step = s);
</script>

{#if step === 'again' && to.again}
  {@const g = to.again}
  <div class="heading">{g.already === g.of ? `${g.p.name} holds ${to.whole}` : `${g.already} of these ${g.of} tracks are in ${g.p.name}`}</div>
  <div role="group" aria-label="Add again">
    <MenuEntry icon="listAdd" label="Add again" busy={to.adding === g.p.id} disabled={to.adding !== null} onclick={() => to.addAgain(g.p, onclose)} />
    {#if g.already < g.of}
      <MenuEntry icon="check" label="Add only the others" disabled={to.adding !== null} onclick={() => to.addOthers(g.p, go)} />
    {/if}
  </div>
{:else}
  <PlaylistToggles title="Add to playlist" playlists={to.playlists} ticked={to.holds}>
    {#snippet entry(p)}
      <MenuEntry label={p.name} checked={to.holds(p)} busy={to.adding === p.id} disabled={to.adding !== null && to.adding !== p.id} onclick={() => to.add(p, go)} />
    {/snippet}
  </PlaylistToggles>
{/if}

<style>
  .heading {
    padding: 0 16px 12px;
    font-size: 18px;
    text-align: center;
  }
</style>
