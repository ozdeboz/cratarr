<script lang="ts">
  // The playlists' menu (docs/ui-conventions.md, decisions 0021, 0025), the same wherever it opens: a long-press on a
  // playlist, the panel's ⋮, or the ⋮ for the ticked ones (one or several). A sheet as Finamp's track menu: the
  // playlist on its card (a mosaic of its first albums' covers; for several, their covers and "3 playlists"), Play,
  // Shuffle, Play next and Add to queue as the large actions (several: one playlist after another, each track once),
  // then View, the favourite, Pin, Export… and Sync to… (they show the panel on that tab), Spoken as… and Rename (one
  // playlist only), and Delete (one Undo for all), each done inside the sheet.
  // Success shows where it happens (the header, the list); what can be undone says so in a toast, to offer Undo
  // (decision 0026).
  import { deletePlaylist, setSpoken, spokenNames, type Link, type PlayMode, incomingCoverUrl, playlistCovers } from './api';
  import MenuEntry from './MenuEntry.svelte';
  import { startPlaylists } from './output.svelte';
  import { viewSelection, panel, type Tab } from './panel.svelte';
  import { followCreate, followDelete, followRename, renameEverywhere } from './follow.svelte';
  import { withUndo } from './undo.svelte';
  import { isPlaylistFavorite, loadPlaylistFavorites, setPlaylistFavorites } from './favourites.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { prefs } from './prefs.svelte';
  import { refresh } from './live.svelte';
  import { toast } from './toast.svelte';

  type Item = { id: string; name: string; count: number; dead: number; links?: Link[]; spoken?: string[] };
  let {
    playlists,
    onclose,
    onrenamed,
    ondeleted,
  }: {
    /** One or more (in the order ticked). */
    playlists: Item[];
    onclose: () => void;
    onrenamed?: (from: string, to: string) => void;
    ondeleted?: (id: string) => void;
  } = $props();

  let mode = $state<'menu' | 'rename' | 'delete' | 'spoken'>('menu');
  $effect(() => void loadPlaylistFavorites());

  /** The one playlist, when the menu is for one. */
  const one = $derived(playlists.length === 1 ? playlists[0] : null);
  const ids = $derived(playlists.map((x) => x.id));
  const n = $derived(playlists.length);
  const title = $derived(one ? one.name : `${n} playlists`);
  const tracks = (k: number) => `${k} ${k === 1 ? 'track' : 'tracks'}`;
  const missing = (k: number) => `${k} missing ${k === 1 ? 'track' : 'tracks'}`;

  // The card's image: one playlist's first four albums (one cover when it has fewer); several: the first cover of each
  // of the first four.
  let covers = $state<string[]>([]);
  $effect(() => {
    const want = ids;
    let gone = false;
    const asked =
      want.length === 1
        ? playlistCovers(want[0]).then((list) => list.map((c) => incomingCoverUrl(c.source, c.album_id)))
        : Promise.all(want.slice(0, 4).map((x) => playlistCovers(x).catch(() => []))).then((lists) =>
            lists.flatMap((l) => (l[0] ? [incomingCoverUrl(l[0].source, l[0].album_id)] : [])),
          );
    asked
      .then((urls) => {
        if (!gone) covers = urls.length >= 4 ? urls.slice(0, 4) : urls.slice(0, 1);
      })
      .catch(() => {});
    return () => (gone = true);
  });

  const allFav = $derived(ids.every((id) => isPlaylistFavorite(id)));
  const allPinned = $derived(ids.every((id) => prefs.pinned.includes(id)));
  /** The panel shows exactly these already. */
  const shown = $derived(panel.selection.join('\n') === ids.join('\n'));

  // Spoken as… (decision 0015): other names a voice assistant may hear for it, comma-separated.
  let spokenField = $state('');
  async function saveSpoken() {
    if (working || !one) return;
    working = true;
    try {
      const { undo } = await setSpoken(one.id, spokenNames(spokenField));
      withUndo(`Saved the names of ${one.name}.`, [undo]);
      onclose();
    } catch (e) {
      fail(e, 'Could not save the names');
    } finally {
      working = false;
      refresh();
    }
  }

  /** View, Export… and Sync to…: the panel shows these playlists, on that tab (decision 0025). */
  function viewTab(tab: Tab) {
    viewSelection([...ids], tab);
    onclose();
  }
  let nameField = $state('');
  let working = $state(false);
  const fail = (e: unknown, what: string) => toast.fail(e, what);

  async function play(how: PlayMode) {
    if (working) return;
    working = true;
    const list = [...ids];
    onclose();
    try {
      // Here when the output is this browser (decision 0022), else on the server's player.
      await startPlaylists(list, how);
    } catch (e) {
      fail(e, 'Could not start playing');
    } finally {
      working = false;
    }
  }

  function pin() {
    prefs.pinned = allPinned ? prefs.pinned.filter((x) => !ids.includes(x)) : [...prefs.pinned, ...ids.filter((x) => !prefs.pinned.includes(x))];
    onclose();
  }

  async function rename() {
    const to = nameField.trim();
    if (!one || !to || to === one.id || working) return;
    working = true;
    try {
      const from = one.id;
      const undo = await renameEverywhere(from, to);
      withUndo(`Renamed ${one.name} to ${to}.`, [undo], { after: (undone) => (undone ? followRename(to, from) : followRename(from, to)) });
      onrenamed?.(from, to);
      onclose();
    } catch (e) {
      fail(e, 'Something went wrong');
    } finally {
      working = false;
      refresh();
    }
  }

  async function remove() {
    if (working) return;
    working = true;
    const list = playlists.map((x) => ({ id: x.id, name: x.name }));
    const gone: string[] = [];
    const tokens: (string | null)[] = [];
    try {
      for (const { id } of list) {
        tokens.push((await deletePlaylist(id)).undo);
        gone.push(id);
        followDelete(id);
        ondeleted?.(id);
      }
      onclose();
    } catch (e) {
      fail(e, 'Something went wrong');
    } finally {
      working = false;
      refresh();
    }
    if (!gone.length) return;
    withUndo(gone.length === 1 ? `Deleted ${list.find((x) => x.id === gone[0])?.name ?? gone[0]}.` : `Deleted ${gone.length} playlists.`, tokens, {
      after: (undone) => gone.forEach(undone ? followCreate : followDelete),
    });
  }
</script>

<MenuSheet
  label={title}
  {onclose}
  images={covers}
  {title}
  chips={one
    ? [tracks(one.count), ...(one.dead > 0 ? [missing(one.dead)] : []), ...(one.links ?? []).map((l) => l.label)]
    : [tracks(playlists.reduce((k, x) => k + x.count, 0)), ...playlists.slice(0, 3).map((x) => x.name), ...(n > 3 ? [`and ${n - 3} more`] : [])]}
  actions={mode === 'menu'
    ? [
        { icon: 'play', label: 'Play', onclick: () => play('play'), disabled: working },
        { icon: 'shuffle', label: 'Shuffle', onclick: () => play('shuffle'), disabled: working },
        { icon: 'playNext', label: 'Play next', onclick: () => play('next'), disabled: working },
        { icon: 'playlists', label: 'Add to queue', onclick: () => play('last'), disabled: working },
      ]
    : []}
  onback={mode === 'menu' ? null : () => (mode = 'menu')}
  step={mode}
>
  {#if mode === 'menu'}
    <div role="menu">
      {#if !shown}
        <MenuEntry icon="squareArrowOutUpRight" label="View" onclick={() => viewTab('tracks')} />
      {/if}
      <MenuEntry
        icon="heart"
        label={allFav ? 'Remove from favourites' : 'Add to favourites'}
        onclick={() => {
          setPlaylistFavorites([...ids], !allFav);
          onclose();
        }}
      />
      <MenuEntry icon="pin" label={allPinned ? 'Unpin' : 'Pin to top'} onclick={pin} />
      <MenuEntry icon="download" label="Export…" onclick={() => viewTab('export')} />
      <MenuEntry icon="link" label="Sync to…" onclick={() => viewTab('sync')} />
      {#if one}
        <MenuEntry
          icon="mic"
          label="Spoken as…"
          onclick={() => {
            spokenField = (one.spoken ?? []).join(', ');
            mode = 'spoken';
          }}
        />
        <MenuEntry
          icon="pencil"
          label="Rename"
          onclick={() => {
            nameField = one.id;
            mode = 'rename';
          }}
        />
      {/if}
      <MenuEntry icon="trash" label="Delete" danger onclick={() => (mode = 'delete')} />
    </div>
  {:else if mode === 'rename' && one}
    <form
      class="form"
      onsubmit={(e) => {
        e.preventDefault();
        rename();
      }}
    >
      <div class="heading">Rename</div>
      <input type="text" aria-label="New name" maxlength="100" bind:value={nameField} />
      <button type="submit" class="primary" disabled={working || !nameField.trim() || nameField.trim() === one.id} aria-busy={working}>Rename</button>
    </form>
  {:else if mode === 'spoken'}
    <form
      class="form"
      onsubmit={(e) => {
        e.preventDefault();
        saveSpoken();
      }}
    >
      <div class="heading">Spoken as</div>
      <input type="text" aria-label="Other names, comma-separated" placeholder="japanese, kokoro" bind:value={spokenField} />
      <button type="submit" class="primary" disabled={working} aria-busy={working}>Save</button>
    </form>
  {:else}
    <div class="form">
      <p class="ask">Delete {title}? You can undo this right after.</p>
      <button class="danger solid" disabled={working} aria-busy={working} onclick={remove}>{one ? 'Delete playlist' : `Delete ${n} playlists`}</button>
    </div>
  {/if}
</MenuSheet>

<style>
  .heading {
    font-size: 18px;
    text-align: center;
  }

  .form {
    display: grid;
    gap: 12px;
    padding: 0 16px 8px;
  }

  .form input[type='text'] {
    min-height: 44px;
    padding: 0 12px;
    background: color-mix(in srgb, var(--bg) 70%, transparent);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
  }

  .ask {
    margin: 0;
    font-size: 0.95rem;
    text-align: center;
  }

  .form button {
    min-height: 44px;
    border: 0;
    border-radius: 10px;
    font: inherit;
    cursor: pointer;
  }

  .form button.primary {
    background: var(--art-accent, var(--accent));
    color: var(--accent-text);
    font-weight: 600;
  }

  .form button.danger.solid {
    background: var(--danger);
    color: #fff;
    font-weight: 600;
  }

  .form button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
