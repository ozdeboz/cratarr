<script lang="ts" module>
  /** What Incoming offers for the album, each a step the album panel asks in its own sheet (IncomingSheet): only those
   *  given are listed. `music`: the library Keep and Replace put it in, by its own name. */
  export type AlbumIncoming = { music?: string; keep?: () => void; replace?: () => void; delete?: () => void };
</script>

<script lang="ts">
  // An album's menu (the ⋮ in the album panel's top bar), after Symfonium's: the album on the card at the top, then
  // its entries on rounded cards, related ones joined: the favourite; Add to playlist (all its tracks, the playlists
  // as toggle cards in a step of the sheet, as a track's); and while it is in Incoming, Keep, Replace and Delete,
  // which close the menu and leave the question to the panel.
  import { albumCoverUrl, setAlbumFavorite, setTrackMember, trackState, type Playlist, type TrackState } from './api';
  import { isFavorite, loadFavorites, setFavorite } from './favourites.svelte';
  import { refresh } from './live.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { toast } from './toast.svelte';

  let {
    source,
    album,
    title,
    artist,
    cover = null,
    tracks,
    incoming = null,
    start = 'menu',
    onclose,
  }: {
    source: string;
    /** The album's id on `source`. */
    album: string;
    title: string;
    artist: string | null;
    /** Its cover; the server's cover of the album when not given. */
    cover?: string | null;
    /** Its tracks' ids on `source`, in play order (the rows that have one). */
    tracks: string[];
    incoming?: AlbumIncoming | null;
    /** The step it opens on: the entries, or straight at the playlists. */
    start?: 'menu' | 'lists';
    onclose: () => void;
  } = $props();

  // The heart is the one the panel's top bar shows (lib/favourites.svelte.ts), so both change together. Asked for
  // again in case the menu opens where the album's heart was not.
  $effect(() => {
    loadFavorites(source, [album]);
  });
  const fav = $derived(isFavorite(source, album));
  // Shows the change at once, back as it was when the server refuses (the panel's heart does the same).
  async function toggleFavourite() {
    const on = !fav;
    setFavorite(source, album, on);
    navigator.vibrate?.(8);
    try {
      await setAlbumFavorite(source, album, on);
    } catch (e) {
      setFavorite(source, album, !on);
      toast.fail(e, "The favourite didn't change");
    }
  }

  // Each track's playlists, by track id. The first track's comes when the menu opens: whether the album is in Music
  // (only then can it go in a playlist) and which playlists there are. The others' come when the playlists show, to
  // tell which already hold the whole album. There is no call for several tracks at once.
  let states = $state<Record<string, TrackState>>({});
  // Asked for already (answered or on its way), outside the state, so an answer does not ask again for the others.
  const asked = new Set<string>();
  let failed = false;
  function ask(ids: string[]) {
    for (const id of ids) {
      if (asked.has(id)) continue;
      asked.add(id);
      trackState(source, id).then(
        (v) => (states[id] = v),
        (e) => {
          // Asked again the next time the playlists show; one line for the lot, not one per track.
          asked.delete(id);
          if (failed) return;
          failed = true;
          toast.fail(e, "This album's playlists couldn't be loaded");
        },
      );
    }
  }
  $effect(() => {
    if (tracks[0]) ask([tracks[0]]);
  });
  const first = $derived(tracks[0] ? (states[tracks[0]] ?? null) : null);
  const addable = $derived(!!first?.addable);

  // svelte-ignore state_referenced_locally
  let step = $state<'menu' | 'lists'>(start);
  $effect(() => {
    if (step === 'lists') ask(tracks);
  });
  // Opened at the playlists for an album outside Music: there is nothing to add it to.
  $effect(() => {
    if (step === 'lists' && first && !first.addable) {
      toast.fail(null, "This album isn't in Music, so it can't go in a playlist");
      onclose();
    }
  });

  const playlists = $derived([...(first?.playlists ?? [])].sort((a, b) => a.name.localeCompare(b.name)));
  /** Every track's state is known (until then no playlist is marked as holding the album). */
  const known = $derived(tracks.every((id) => states[id]));
  const member = (id: string, p: Playlist) => !!states[id]?.playlists.find((x) => x.id === p.id)?.member;
  /** The playlist holds every track of the album that can go in one. */
  const holds = (p: Playlist) => known && tracks.every((id) => !states[id].addable || member(id, p));

  // The playlist the album is going into: one at a time, its card spinning until the last track is in.
  let adding = $state<string | null>(null);
  async function add(p: Playlist) {
    if (adding || holds(p)) return;
    adding = p.id;
    try {
      // One by one, in play order, so the tracks are added in it; those already in it (or not in Music) are passed.
      for (const id of tracks) {
        const s = states[id];
        if (s && (!s.addable || member(id, p))) continue;
        states[id] = await setTrackMember(source, id, p.id, true);
      }
      refresh();
    } catch (e) {
      toast.fail(e, 'Not every track was added');
      refresh();
    } finally {
      adding = null;
    }
  }

  /** An Incoming step: the menu closes and the panel asks its question. */
  function incomingStep(run: () => void) {
    onclose();
    run();
  }
  const music = $derived(incoming?.music ?? 'Music');
</script>

<MenuSheet
  label={title}
  {onclose}
  images={[cover ?? albumCoverUrl(source, album)]}
  {title}
  chips={[artist ?? 'Unknown artist']}
  onback={step === 'menu' ? null : () => (step = 'menu')}
  {step}
>
  {#if step === 'menu'}
    <div class="cards" role="menu">
      <div class="group" role="group">
        <MenuEntry icon="heart" label={fav ? 'Remove from favourites' : 'Add to favourites'} onclick={toggleFavourite} />
      </div>
      {#if addable}
        <div class="group" role="group">
          <MenuEntry icon="listAdd" label="Add to playlist" onclick={() => (step = 'lists')} />
        </div>
      {/if}
      {#if incoming && (incoming.keep || incoming.replace || incoming.delete)}
        {@const { keep, replace, delete: remove } = incoming}
        <div class="group" role="group" aria-label="Incoming">
          {#if keep}<MenuEntry icon="check" label={`Keep in ${music}`} onclick={() => incomingStep(keep)} />{/if}
          {#if replace}<MenuEntry icon="swap" label={`Replace in ${music}`} onclick={() => incomingStep(replace)} />{/if}
          {#if remove}<MenuEntry icon="trash" label="Delete album" danger onclick={() => incomingStep(remove)} />{/if}
        </div>
      {/if}
    </div>
  {:else}
    <div class="heading">Add to playlist</div>
    <div role="group" aria-label="Playlists">
      {#each playlists as p (p.id)}
        <MenuEntry
          label={p.name}
          checked={holds(p)}
          busy={adding === p.id}
          disabled={adding !== null && adding !== p.id}
          onclick={() => add(p)}
        />
      {/each}
    </div>
  {/if}
</MenuSheet>

<style>
  /* Symfonium's menu: the entries on rounded cards, a little apart; related ones joined in one card, each a row of it
     a hairline apart, the card's corners on the first and last. */
  .cards {
    display: grid;
    gap: 12px;
    padding: 4px 12px 8px;
  }

  .group {
    display: grid;
    gap: 3px;
  }

  .group > :global(.entry) {
    min-height: 60px;
    padding: 0 20px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  .group > :global(.entry:hover),
  .group > :global(.entry:focus-visible) {
    background: color-mix(in srgb, var(--text) 13%, transparent);
  }

  .group > :global(.entry:first-child) {
    border-top-left-radius: 20px;
    border-top-right-radius: 20px;
  }

  .group > :global(.entry:last-child) {
    border-bottom-left-radius: 20px;
    border-bottom-right-radius: 20px;
  }

  .heading {
    padding: 0 16px 12px;
    font-size: 18px;
    text-align: center;
  }
</style>
