<script lang="ts" module>
  /** A track the menu is for, when it is not the playing one (a queue row, a playlist row; decision 0023). */
  export type MenuTrack = { source: string; id: string; name: string; artist: string | null; album: string | null; album_id: string | null };
</script>

<script lang="ts">
  // A track's menu (decisions 0019, 0021, 0023): the playing track's (the menu button in the player, a right-click on
  // its cover) or any other's (a long-press on a row). A sheet as Finamp's track menu: the track on the card (with the
  // heart), Play next and Add to queue as the large actions, then: Add to playlist (the playlists as toggle cards, a tap
  // adds or removes), the favourite, and for the playing track a new playlist made of what plays and what comes next.
  import { coverUrl, incomingCoverUrl, playlistFromQueue, queueTrack, setTrackFavorite, setTrackMember, trackState, type TrackState } from './api';
  import { refresh } from './live.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { shownCover, favorite, pending, playing, toggleFavorite } from './remote.svelte';
  import { enqueue, local } from './localPlayer.svelte';
  import { isLocal, setPlayingMember, shownPlaylists } from './output.svelte';
  import { setFavorite } from './favourites.svelte';
  import { toast } from './toast.svelte';

  let {
    onclose,
    track = null,
    start = 'menu',
  }: { onclose: () => void; track?: MenuTrack | null; start?: 'menu' | 'lists' } = $props();

  const np = $derived(playing());
  /** The menu is for what plays (no track given, or the given one plays). */
  const forPlaying = $derived(!track || (!!np && np.source === track.source && np.id === track.id));
  const t = $derived<MenuTrack | null>(forPlaying ? np : track);

  // Another track's playlists and heart, asked for when its menu opens.
  let other = $state<TrackState | null>(null);
  $effect(() => {
    if (forPlaying || !track) return;
    const { source, id } = track;
    trackState(source, id).then(
      (v) => (other = v),
      (e) => toast.fail(e, "This track's playlists couldn't be loaded"),
    );
  });

  const src = $derived(
    forPlaying ? shownCover() : track ? (track.album_id ? incomingCoverUrl(track.source, track.album_id) : coverUrl(track)) : null,
  );
  const addable = $derived(forPlaying ? !!np?.addable : !!other?.addable);
  const heartOn = $derived(forPlaying ? favorite() : !!other?.favorite);
  const heartKnown = $derived(forPlaying ? np?.favorite !== null : other !== null && other.favorite !== null);
  const busy = $derived(forPlaying ? pending() : !other);
  // Playlists holding the track first, then the rest by name.
  const playlists = $derived(
    [...(forPlaying ? shownPlaylists() : (other?.playlists ?? []))].sort(
      (a, b) => Number(b.member) - Number(a.member) || a.name.localeCompare(b.name),
    ),
  );

  // The step it opens on; later steps are the sheet's own.
  // svelte-ignore state_referenced_locally
  let step = $state<'menu' | 'lists' | 'new'>(start);
  let working = $state(false);
  let nameField = $state('');

  async function heart() {
    if (forPlaying) return toggleFavorite();
    if (!track || !other || other.favorite === null) return;
    try {
      other = await setTrackFavorite(track.source, track.id, !other.favorite);
      // The rows' hearts show it too.
      if (other.favorite !== null) setFavorite(track.source, track.id, other.favorite);
      refresh();
    } catch (e) {
      toast.fail(e, "The favourite didn't change");
    }
  }

  async function queue(how: 'next' | 'last') {
    if (!t) return;
    working = true;
    onclose();
    try {
      if (isLocal()) enqueue([{ source: t.source, id: t.id, name: t.name, artist: t.artist, album: t.album, album_id: t.album_id }], how);
      else await queueTrack(t.id, how, t.source);
      toast.show({ text: how === 'next' ? 'Plays next.' : 'Added to the queue.', tone: 'done' });
      setTimeout(refresh, 700);
    } catch (e) {
      toast.fail(e, how === 'next' ? "It couldn't be put next" : "It couldn't be added to the queue");
    } finally {
      working = false;
    }
  }

  async function toggle(id: string, member: boolean) {
    if (!t || busy) return;
    try {
      if (forPlaying) await setPlayingMember(id, !member);
      else if (track) {
        other = await setTrackMember(track.source, track.id, id, !member);
        refresh();
      }
    } catch (e) {
      toast.fail(e, member ? "It couldn't be taken out" : "It couldn't be added");
    }
  }

  async function makePlaylist() {
    const name = nameField.trim();
    if (!name || working) return;
    working = true;
    try {
      const ids = isLocal() ? local.queue.slice(Math.max(0, local.index)).map((t) => t.id) : undefined;
      const made = await playlistFromQueue(name, t?.source, ids);
      const left = made.left_out ? `, ${made.left_out} not in the library left out` : '';
      toast.show({ text: `Made ${made.playlist}: ${made.added} ${made.added === 1 ? 'track' : 'tracks'}${left}.`, tone: 'done' });
      onclose();
      refresh();
    } catch (e) {
      toast.fail(e, "The playlist wasn't made");
    } finally {
      working = false;
    }
  }
</script>

{#if t}
  <MenuSheet
    label={t.name}
    {onclose}
    images={src ? [src] : []}
    title={t.name}
    chips={[t.artist, t.album].filter((c): c is string => !!c)}
    heart={heartKnown ? { on: heartOn, disabled: busy, toggle: heart } : null}
    actions={step === 'menu'
      ? [
          { icon: 'playNext', label: 'Play next', onclick: () => queue('next'), disabled: working },
          { icon: 'playlists', label: 'Add to queue', onclick: () => queue('last'), disabled: working },
        ]
      : []}
    onback={step === 'menu' ? null : () => (step = 'menu')}
    {step}
  >
    {#if step === 'menu'}
      <div role="menu">
        {#if addable}
          <MenuEntry icon="listAdd" label="Add to playlist" onclick={() => (step = 'lists')} />
        {/if}
        {#if heartKnown}
          <MenuEntry icon="heart" label={heartOn ? 'Remove from favourites' : 'Add to favourites'} disabled={busy} onclick={heart} />
        {/if}
        {#if forPlaying}
          <MenuEntry
            icon="plus"
            label="New playlist from the queue"
            onclick={() => {
              nameField = '';
              step = 'new';
            }}
          />
        {/if}
      </div>
    {:else if step === 'lists'}
      <div class="heading">Add to / remove from playlists</div>
      <div role="group" aria-label="Playlists">
        {#each playlists as p (p.id)}
          <MenuEntry label={p.name} checked={p.member} disabled={busy} onclick={() => toggle(p.id, p.member)} />
        {/each}
      </div>
    {:else}
      <form
        class="form"
        onsubmit={(e) => {
          e.preventDefault();
          makePlaylist();
        }}
      >
        <div class="heading">New playlist from the queue</div>
        <input type="text" aria-label="Name of the new playlist" placeholder="Name" maxlength="100" bind:value={nameField} />
        <button type="submit" class="primary" disabled={working || !nameField.trim()} aria-busy={working}>Make playlist</button>
      </form>
    {/if}
  </MenuSheet>
{/if}

<style>
  .heading {
    padding: 0 16px 12px;
    font-size: 18px;
    text-align: center;
  }

  .form {
    display: grid;
    gap: 12px;
    padding: 0 16px 8px;
  }

  .form .heading {
    padding: 0 0 4px;
  }

  .form input {
    min-height: 44px;
    padding: 0 12px;
    background: color-mix(in srgb, var(--bg) 70%, transparent);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
  }

  .primary {
    min-height: 44px;
    border: 0;
    border-radius: 10px;
    background: var(--art-accent, var(--accent));
    color: var(--accent-text);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .primary:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
