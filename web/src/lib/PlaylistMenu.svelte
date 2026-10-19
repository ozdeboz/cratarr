<script lang="ts">
  // The playlists' menu (docs/ui-conventions.md, decisions 0021, 0025), the same wherever it opens: a long-press on a
  // playlist, the panel's ⋮, or the ⋮ for the ticked ones (one or several). A sheet as Finamp's track menu: the
  // playlist on its card (a mosaic of its first albums' covers; for several, their covers and "3 playlists"), Play,
  // Shuffle, Play next and Add to queue as the large actions (several: one playlist after another, each track once),
  // then View, the favourite, Pin, Export… and Sync… (they show the panel on that tab), Spoken as… and Rename (one
  // playlist only), and Delete (one Undo for all), each done inside the sheet. Since several users (decision 0051): one's
  // own playlist can be shared with the other users; another user's shared one shows whose it is, and is copied rather
  // than changed (nor repaired); Sync… is the administrators'.
  // Success shows where it happens (the header, the list); what can be undone says so in a toast, to offer Undo
  // (decision 0026).
  import { linkTo, share } from './share';
  import { untrack } from 'svelte';
  import { copyPlaylist, deletePlaylist, setPlaylistShared, setSpoken, spokenNames, type Link, type PlayMode, albumCoverUrl, playlistCovers, playlistTrackIds } from './api';
  import { me } from './me.svelte';
  import { startRadio } from './localPlayer.svelte';
  import { isKept, setKept } from './kept.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import { isLocal, startPlaylists } from './output.svelte';
  import { openTab, viewSelection, panel, type Tab } from './panel.svelte';
  import { followCreate, followDelete, followRename, renameEverywhere } from './follow.svelte';
  import { withUndo } from './undo.svelte';
  import { isPlaylistFavorite, loadPlaylistFavorites, setPlaylistFavorites } from './favourites.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { prefs } from './prefs.svelte';
  import { refresh } from './live.svelte';
  import { toast } from './toast.svelte';

  type Item = { id: string; name: string; count: number; dead: number; links?: Link[]; spoken?: string[]; owner?: string; shared?: boolean; mine?: boolean };
  let {
    playlists,
    onclose,
    onrenamed,
    ondeleted,
    ondownload = null,
    keepFrom = null,
    start = 'menu',
  }: {
    /** One or more (in the order ticked). */
    playlists: Item[];
    /** Straight to Delete's confirmation (the edit bar's trash). */
    start?: 'menu' | 'delete';
    onclose: () => void;
    onrenamed?: (from: string, to: string) => void;
    ondeleted?: (id: string) => void;
    /** Its tracks onto the phone (the app's Downloads); given only in the app, for one playlist whose rows are known. */
    ondownload?: (() => void) | null;
    /** The server its tracks are kept from (Keep on this device, decision 0038): given in the app, for one playlist. */
    keepFrom?: string | null;
  } = $props();

  let mode = $state<'menu' | 'rename' | 'delete' | 'spoken'>(untrack(() => start));
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
        ? playlistCovers(want[0]).then((list) => list.map((c) => albumCoverUrl(c.source, c.album_id)))
        : Promise.all(want.slice(0, 4).map((x) => playlistCovers(x).catch(() => []))).then((lists) =>
            lists.flatMap((l) => (l[0] ? [albumCoverUrl(l[0].source, l[0].album_id)] : [])),
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

  /** View, Export… and Sync…: the panel shows these playlists, on that tab (decision 0025). */
  function viewTab(tab: Tab) {
    if (tab === 'tracks') viewSelection([...ids]);
    else openTab([...ids], tab);
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

  /** Radio from these playlists (decision 0036): tracks like theirs, none of their own, and the radio goes on. */
  async function radio() {
    if (working) return;
    working = true;
    const list = [...ids];
    onclose();
    try {
      const seed = new Set<string>();
      for (const id of list) for (const t of (await playlistTrackIds(id, 'cratarr')).ids) seed.add(t);
      await startRadio([...seed], one?.name ?? null);
    } catch (e) {
      fail(e, "The radio didn't start");
    } finally {
      working = false;
    }
  }

  // Another user's among them: only its owner changes it.
  const others = $derived(playlists.some((x) => x.mine === false));

  async function setShared(id: string, on: boolean) {
    try {
      await setPlaylistShared(id, on);
      onclose();
    } catch (e) {
      fail(e, 'Something went wrong');
    } finally {
      refresh();
    }
  }

  async function copy(id: string) {
    if (working) return;
    working = true;
    try {
      const { playlist, undo } = await copyPlaylist(id);
      withUndo(`Copied to ${playlist}.`, [undo], { after: (undone) => (undone ? followDelete(playlist) : followCreate(playlist)) });
      onclose();
    } catch (e) {
      fail(e, 'Something went wrong');
    } finally {
      working = false;
      refresh();
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
    ? [
        tracks(one.count),
        ...(one.dead > 0 ? [missing(one.dead)] : []),
        ...(one.mine === false && one.owner ? [one.owner] : []),
        ...(one.shared || (one.links ?? []).some((l) => l.target === 'shared') ? ['Shared'] : []),
      ]
    : [tracks(playlists.reduce((k, x) => k + x.count, 0)), ...playlists.slice(0, 3).map((x) => x.name), ...(n > 3 ? [`and ${n - 3} more`] : [])]}
  actions={mode === 'menu'
    ? [
        { icon: 'play', label: 'Play', onclick: () => play('play'), disabled: working },
        { icon: 'shuffle', label: 'Shuffle', onclick: () => play('shuffle'), disabled: working },
        { icon: 'playNext', label: 'Play next', onclick: () => play('next'), disabled: working },
        { icon: 'listAdd', label: 'Add to queue', onclick: () => play('last'), disabled: working },
        ...(isLocal() ? [{ icon: 'radio' as const, label: 'Start radio', onclick: radio, disabled: working }] : []),
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
      {#if ondownload}
        <MenuEntry
          icon="arrowDownToLine"
          label="Download"
          onclick={() => {
            onclose();
            ondownload();
          }}
        />
      {/if}
      {#if one && keepFrom}
        {@const from = keepFrom}
        {@const p = one}
        {@const on = isKept('playlist', from, p.id)}
        <MenuEntry icon="download" label="Keep on this device" switched={on} onclick={() => setKept('playlist', from, p.id, !on, p.name)} />
      {/if}
      {#if one}
        <!-- Titled by its name; linked by its id (another user's shared one is "owner/name"). -->
        {@const pid = one.id}
        {@const name = one.name}
        <MenuEntry
          icon="share"
          label="Share"
          onclick={() => {
            onclose();
            share(name, name, linkTo('playlists', pid));
          }}
        />
      {/if}
      {#if one && one.mine === false}
        {@const id = one.id}
        <MenuEntry icon="copyCheck" label="Copy to my playlists" busy={working} disabled={working} onclick={() => copy(id)} />
      {/if}
      <!-- Sharing needs the permission to share; one shared before can always be unshared. -->
      {#if one && one.mine !== false && one.shared !== undefined && (me.permissions.share || one.shared)}
        {@const p = one}
        <MenuEntry icon="users" label="Shared with other users" switched={!!p.shared} onclick={() => setShared(p.id, !p.shared)} />
      {/if}
      <!-- The panel's other views of them (back returns to their tracks). -->
      <MenuEntry icon="info" label="Info" onclick={() => viewTab('info')} />
      <MenuEntry icon="download" label="Export…" onclick={() => viewTab('export')} />
      {#if !others}
        <MenuEntry icon="repairs" label="Repair…" onclick={() => viewTab('repair')} />
        {#if me.admin}<MenuEntry icon="link" label="Sync…" onclick={() => viewTab('sync')} />{/if}
      {/if}
      {#if one && !others}
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
      {#if !others}
        <MenuEntry icon="trash" label={one ? 'Delete playlist' : `Delete ${ids.length} playlists`} danger onclick={() => (mode = 'delete')} />
      {/if}
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
