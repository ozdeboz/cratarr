<script lang="ts">
  // The playlists' menu (docs/ui-conventions.md, decisions 0021, 0025), the same wherever it opens: a long-press on a
  // playlist, the panel's ⋮, or the ⋮ for the ticked ones (one or several). A sheet as Finamp's track menu: the
  // playlist on its card (a mosaic of its first albums' covers; for several, their covers and "3 playlists"), Play,
  // Shuffle and Start radio as the large actions (several: one playlist after another, each track once), then in the
  // table's order (lib/menuTable) View, the favourite, Pin, Download… (to this device in the app: all their tracks, each
  // once; or as files) and Sync to this device (each of them), Info, Repair… and Sync… (they show the panel on that tab),
  // Rename and Spoken as… (one playlist only), and Delete (one Undo for all), each done inside the sheet.
  // Since several users (decision 0051): one's own playlist can be shared with the other users; another user's shared
  // one shows whose it is, and is copied rather than changed (nor repaired); Sync… is the administrators'. Ticked with
  // others' (the owner, 2026-10-27: what one has, several have): Copy to my playlists copies the others' (one Undo),
  // Repair…, Sync… and Delete take one's own.
  // Success shows where it happens (the header, the list); what can be undone says so in a toast, to offer Undo
  // (decision 0026).
  import { batchDelete, batchRows, missingCall, playlistRows, type PlaylistRow } from './api';
  import { port } from './player/port.svelte';
  import { inApp } from './app.svelte';
  import { trackSteps } from './progress.svelte';
  import { linkTo, share } from './share';
  import { untrack } from 'svelte';
  import { copyPlaylist, deletePlaylist, setPlaylistShared, setSpoken, spokenNames, type Link, type PlayMode, albumCoverUrl, playlistCovers, playlistTrackIds } from './api';
  import { me } from './me.svelte';
  import { startRadio } from './localPlayer.svelte';
  import { isKept, setKept } from './kept.svelte';
  import type { Downloadable } from './app.svelte';
  import DownloadStep from './DownloadStep.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuTable from './MenuTable.svelte';
  import { inOrder, type EntryId } from './menuTable';
  import { installed } from './offline.svelte';
  import { startPlaylists } from './output.svelte';
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
    downloadable = null,
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
    /** Its tracks, for Download… to this device (in the app, by an opener that has its rows: one playlist viewed); else
     *  asked for. */
    downloadable?: Downloadable[] | null;
    /** The server its tracks are kept from (Sync to this device, decision 0038), given with `downloadable`; else as the
     *  rows asked for say. */
    keepFrom?: string | null;
  } = $props();

  let mode = $state<'menu' | 'rename' | 'delete' | 'spoken' | 'download'>(untrack(() => start));
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

  /** View, Info, Repair… and Sync…: the panel shows these playlists, on that tab (decision 0025). */
  function viewTab(tab: Tab, which = ids) {
    if (tab === 'tracks') viewSelection([...which]);
    else openTab([...which], tab);
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
  /** One's own of them (Repair…, Sync…, Delete) and the others' (Copy to my playlists). */
  const own = $derived(playlists.filter((x) => x.mine !== false));
  const theirs = $derived(playlists.filter((x) => x.mine === false));
  const ownIds = $derived(own.map((x) => x.id));

  // In the app (and an installed page), where the opener has not their rows (several, the Playlists page): asked for
  // once (one call where the server has it, 0.9.4), for Download… to this device (all their tracks, each once) and Sync
  // to this device (each from its own server).
  let asked = $state<{ id: string; source: string; rows: PlaylistRow[] }[] | null>(null);
  const idsKey = $derived(ids.join('\n'));
  $effect(() => {
    if (!(inApp || installed) || downloadable) return;
    const want = idsKey.split('\n');
    const from = prefs.source || undefined;
    let gone = false;
    batchRows(want, from)
      .then((r) => r.playlists.flatMap((x) => ('rows' in x ? [x] : [])))
      .catch((e) => {
        if (!missingCall(e)) throw e;
        return Promise.all(want.map((id) => playlistRows(id, from).then((x) => ({ id, ...x }))));
      })
      .then(
        (l) => void (!gone && (asked = l)),
        () => {},
      );
    return () => (gone = true);
  });
  const tracksToGet = $derived.by((): Downloadable[] | null => {
    if (downloadable) return downloadable;
    if (!asked) return null;
    const seen = new Set<string>();
    return asked.flatMap((p) =>
      p.rows.flatMap((r) => {
        const k = `${p.source}/${r.id}`;
        if (!r.id || r.missing || seen.has(k)) return [];
        seen.add(k);
        return [{ source: p.source, id: r.id, name: r.name ?? r.file, artist: r.artist, album: r.album, album_id: r.album_id ?? null }];
      }),
    );
  });
  /** Each playlist and the server it is kept from. */
  const keepable = $derived(
    !inApp
      ? []
      : one && keepFrom
        ? [{ p: one, from: keepFrom }]
        : (asked ?? []).flatMap((a) => playlists.filter((p) => p.id === a.id).map((p) => ({ p, from: a.source }))),
  );
  const allKept = $derived(keepable.length > 0 && keepable.every((k) => isKept('playlist', k.from, k.p.id)));

  // Shared with the other users: every one of them, ticked several too (the owner, 2026-10-27: it was one's alone).
  const shareable = $derived(!others && playlists.every((x) => x.shared !== undefined));
  const allShared = $derived(playlists.every((x) => x.shared));
  async function setShared(ids: string[], on: boolean) {
    try {
      for (const id of ids) await setPlaylistShared(id, on);
      onclose();
    } catch (e) {
      fail(e, 'Something went wrong');
    } finally {
      refresh();
    }
  }

  /** The others' ticked ones into one's own playlists, one Undo for them all. */
  async function copy(list: string[]) {
    if (working) return;
    working = true;
    const made: string[] = [];
    const tokens: (string | null)[] = [];
    try {
      for (const id of list) {
        const { playlist, undo } = await copyPlaylist(id);
        made.push(playlist);
        tokens.push(undo);
      }
      onclose();
    } catch (e) {
      fail(e, 'Something went wrong');
    } finally {
      working = false;
      refresh();
    }
    if (!made.length) return;
    withUndo(made.length === 1 ? `Copied to ${made[0]}.` : `Copied ${made.length} playlists.`, tokens, {
      after: (undone) => made.forEach(undone ? followDelete : followCreate),
    });
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
    const list = own.map((x) => ({ id: x.id, name: x.name }));
    const gone: string[] = [];
    const tokens: (string | null)[] = [];
    try {
      // One call where the server has it (0.9.4): one Undo for them all; else one by one.
      let batched = false;
      try {
        const r = await batchDelete(list.map((x) => x.id));
        tokens.push(...r.undo);
        for (const id of r.deleted) {
          gone.push(id);
          followDelete(id);
          ondeleted?.(id);
        }
        batched = true;
        if (r.failed.length) throw new Error(r.failed[0].error);
      } catch (e) {
        if (batched || !missingCall(e)) throw e;
      }
      if (!batched) await trackSteps(list.length, async (done) => {
        for (const { id } of list) {
          tokens.push((await deletePlaylist(id)).undo);
          gone.push(id);
          followDelete(id);
          ondeleted?.(id);
          done();
        }
      });
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

{#snippet entry(e: EntryId)}
  {#if e === 'view'}
    {#if !shown}<MenuEntry icon="squareArrowOutUpRight" label="View" onclick={() => viewTab('tracks')} />{/if}
  {:else if e === 'favourite'}
    <MenuEntry
      icon="heart"
      label="Favourite"
      switched={allFav}
      onclick={() => {
        setPlaylistFavorites([...ids], !allFav);
        onclose();
      }}
    />
  {:else if e === 'pin'}
    <MenuEntry icon="pin" label={allPinned ? 'Unpin' : 'Pin to top'} onclick={pin} />
  {:else if e === 'copyToMine'}
    {#if theirs.length}
      <MenuEntry icon="copyCheck" label="Copy to my playlists" busy={working} disabled={working} onclick={() => copy(theirs.map((x) => x.id))} />
    {/if}
  {:else if e === 'download'}
    <MenuEntry icon="arrowDownToLine" label="Download…" onclick={() => (mode = 'download')} />
  {:else if e === 'keep'}
    {#if keepable.length && keepable.length === n}
      <!-- All of them on, or all off when all are. -->
      <MenuEntry
        icon="download"
        label="Sync to this device"
        switched={allKept}
        onclick={() => {
          const on = !allKept;
          for (const k of keepable) setKept('playlist', k.from, k.p.id, on, k.p.name);
        }}
      />
    {/if}
  {:else if e === 'share'}
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
  {:else if e === 'sharedWithUsers'}
    <!-- Sharing needs the permission to share; one shared before can always be unshared. -->
    {#if shareable && (me.permissions.share || allShared)}
      <MenuEntry icon="users" label="Shared with other users" switched={allShared} onclick={() => setShared(ids, !allShared)} />
    {/if}
  {:else if e === 'info'}
    <!-- The panel's other views of them (back returns to their tracks). -->
    <MenuEntry icon="info" label="Info" onclick={() => viewTab('info')} />
  {:else if e === 'rename'}
    {#if one && !others}
      <MenuEntry
        icon="pencil"
        label="Rename"
        onclick={() => {
          nameField = one.id;
          mode = 'rename';
        }}
      />
    {/if}
  {:else if e === 'spokenAs'}
    {#if one && !others}
      <MenuEntry
        icon="mic"
        label="Spoken as…"
        onclick={() => {
          spokenField = (one.spoken ?? []).join(', ');
          mode = 'spoken';
        }}
      />
    {/if}
  {:else if e === 'repair'}
    <!-- With others' ticked: one's own only. -->
    {#if own.length}<MenuEntry icon="repairs" label="Repair…" onclick={() => viewTab('repair', ownIds)} />{/if}
  {:else if e === 'sync'}
    {#if own.length && me.admin}<MenuEntry icon="link" label="Sync…" onclick={() => viewTab('sync', ownIds)} />{/if}
  {:else if e === 'deletePlaylist'}
    {#if own.length}
      <MenuEntry icon="trash" label={others ? `Delete ${own.length} of mine` : one ? 'Delete playlist' : `Delete ${n} playlists`} danger onclick={() => (mode = 'delete')} />
    {/if}
  {/if}
{/snippet}

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
    ? inOrder('playlist', [
        { icon: 'play', label: 'Play', onclick: () => play('play'), disabled: working },
        { icon: 'shuffle', label: 'Shuffle', onclick: () => play('shuffle'), disabled: working },
        ...(port().radio ? [{ icon: 'radio' as const, label: 'Start radio', onclick: radio, disabled: working }] : []),
      ])
    : []}
  onback={mode === 'menu' ? null : () => (mode = 'menu')}
  step={mode}
>
  {#if mode === 'menu'}
    <!-- The shared groups, in the table's order (decision 0058, lib/menuTable). -->
    <MenuTable kind="playlist" {entry} />
  {:else if mode === 'download'}
    <DownloadStep what={{ kind: 'playlists', names: [...ids] }} tracks={tracksToGet} {onclose} />
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
      <p class="ask">Delete {own.length === 1 ? own[0].name : `${own.length} playlists`}? You can undo this right after.</p>
      <button class="danger solid" disabled={working || !own.length} aria-busy={working} onclick={remove}>{own.length === 1 ? 'Delete playlist' : `Delete ${own.length} playlists`}</button>
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
    border-radius: 999px;
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
