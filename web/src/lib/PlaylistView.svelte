<script lang="ts">
  import { untrack } from 'svelte';
  // A viewed playlist (decision 0023), in the panel beside the page or in its own layer, styled as Finamp's playlist
  // screen (lib/components/AlbumScreen) in less room: a bar with ✕ and the menu; the covers beside the name, "N tracks
  // (length)" and the genres; Play, Shuffle, Play next and Add to queue as icons over labels; what Insights knows of it
  // (Related: the Info tab); then the tracks in file order. A tap on a track plays the playlist from
  // there on the chosen output, a swipe left plays it next, a long-press is its menu; the heart adds it to playlists
  // (long-press: favourite). Lines the server does not know, or files that are missing, show by file name, dimmed.
  // Edit mode (the pencil; Finamp's playlist edit screen): rows get the reorder handle and ✕ (a swipe left removes too),
  // removed tracks wait in their own section to be restored, and Save writes the playlist once (Cancel drops it).
  // A crate (decision 0024, `?view=crate:…`) shows the same way, without the statistics (genres, length, Related) and
  // without editing; its rows come in pages (newest first), and the save icon makes a playlist of it (a snapshot).
  // Playlists show in tabs (decision 0025): Tracks, Info, Export, Repair, Sync, under the header, for the selection: one
  // playlist as above, or several (the Playlists page's Select): their covers and count in the header, and on Tracks a
  // section per playlist, in the order ticked, each folding, read-only. A crate has no tabs.
  // An album (`?view=album:<server>/<id>`, from the Incoming page or a track's View album) shows as a crate does, its
  // tracks in play order, with all four actions: an album, like a crate, is a list of tracks but no playlist file.
  import {
    albumView,
    coverUrl,
    crateRows,
    crateToPlaylist,
    editPlaylist,
    albumCoverUrl,
    playlistCovers,
    playlistRows,
    queueTrack,
    queueTracks,
    type PlaylistRow,
  } from './api';
  import { crateName, crateOf, crates, cycleFavourites, EDIT_CRATE, FAVOURITES, favouritesOf, favouritesSaved, loadCrates } from './crates.svelte';
  import { Reorder } from './reorder.svelte';
  import { followCreate, followDelete, followRename, renameEverywhere } from './follow.svelte';
  import { undos, withUndo } from './undo.svelte';
  import { isPlaylistFavorite, loadFavorites, loadPlaylistFavorites, setPlaylistFavorites } from './favourites.svelte';
  import Icon, { type IconName } from './Icon.svelte';
  import { insights, plural } from './insights.svelte';
  import { refresh } from './live.svelte';
  import { enqueue, type LocalTrack } from './localPlayer.svelte';
  import { longpress } from './longpress';
  import { isLocal, playPlaylistFrom, shownPlaylists, startPlaylists } from './output.svelte';
  import { albumOf, closePlaylist, viewPlaylist, panel, setTab, TABS } from './panel.svelte';
  import { inSidebar } from './live.svelte';
  import ExportTab from './ExportTab.svelte';
  import PlaylistItem from './PlaylistItem.svelte';
  import Row from './Row.svelte';
  import TrackCover from './TrackCover.svelte';
  import Section from './Section.svelte';
  import { fromAction } from 'svelte/attachments';
  import RepairTab from './RepairTab.svelte';
  import SyncTab from './SyncTab.svelte';
  import PlaylistMenu from './PlaylistMenu.svelte';
  import PanelPlace from './PanelPlace.svelte';
  import { work } from './panelWork.svelte';
  import { prefs } from './prefs.svelte';
  import { clockText, paused, playing } from './remote.svelte';
  import { on } from './events.svelte';
  import RowHeart from './RowHeart.svelte';
  import { swipe } from './swipe';
  import { toast } from './toast.svelte';
  import TrackMenu, { type MenuTrack } from './TrackMenu.svelte';
  import type { PlayMode } from './api';

  const id = $derived(panel.viewed ?? '');
  /** The viewed crate's id, when a crate is viewed (not a playlist). */
  const crate = $derived(crateOf(panel.viewed));
  /** The viewed album (an Incoming one, or a track's), when an album is viewed. */
  const album = $derived(albumOf(panel.viewed));
  /** A crate or an album: tracks without a playlist's file (no tabs, no editing, no statistics). */
  const plain = $derived(!!crate || !!album);
  /** What the album's tracks say of it: its name, artist and year. */
  let albumHead = $state<{ title: string; artist: string | null; year: number | null } | null>(null);
  /** An album's or a crate's genres, most shared first, as its first page came (a playlist's are its Insights'). */
  let listGenres = $state<string[] | null>(null);
  /** The selected playlists; several: Tracks shows a section per playlist (decision 0025). */
  const ids = $derived(panel.selection);
  const multi = $derived(ids.length > 1);
  const tab = $derived(plain ? 'tracks' : panel.tab);
  const title = $derived(crate ? crateName(crate) : album ? (albumHead?.title ?? '') : multi ? `${ids.length} playlists` : id);
  const isPermanentCrate = $derived(!!crate && (crates.list?.find((c) => c.id === crate)?.permanent ?? true));
  const p = $derived(plain || multi ? null : (shownPlaylists().find((x) => x.id === id) ?? null));
  /** The selected playlists, in the order ticked (for the menu, the counts). */
  const selected = $derived(ids.flatMap((x) => shownPlaylists().filter((y) => y.id === x)));
  const chosen = $derived(multi ? selected : []);
  const chosenDead = $derived(chosen.reduce((n, x) => n + x.dead, 0));
  // The heart: on when every selected playlist is a favourite; a tap sets all of them.
  const allFav = $derived(ids.length > 0 && ids.every((x) => isPlaylistFavorite(x)));
  // The pin (Pin to top, as in the menu): on when every selected playlist is pinned; a tap pins or unpins all of them.
  const allPinned = $derived(ids.length > 0 && ids.every((x) => prefs.pinned.includes(x)));
  function togglePins() {
    prefs.pinned = allPinned ? prefs.pinned.filter((x) => !ids.includes(x)) : [...prefs.pinned, ...ids.filter((x) => !prefs.pinned.includes(x))];
  }
  const chosenCount = $derived(chosen.reduce((n, x) => n + x.count, 0));
  const np = $derived(playing());
  /** A crate's size (its rows come in pages). */
  let total = $state(0);
  const PAGE = 200;
  // A crate changes as tracks are sorted into playlists (Unlisted) or hearted: asked for again when the playlists change.
  const listsKey = $derived(crate ? shownPlaylists().map((x) => x.count).join(',') : '');
  // An Incoming album changes as Lidarr adds to it (or it is kept or deleted elsewhere): asked for again when Incoming
  // changes.
  let incomingChanged = $state(0);
  $effect(() => on<{ area: string }>('changed', ({ area }) => area === 'incoming' && album && incomingChanged++));

  // The rows: asked for when the playlist is viewed and again when it changes (its counts, from the shell's state).
  let rows = $state<PlaylistRow[] | null>(null);
  let source = $state('');
  let error = $state<string | null>(null);
  let shownFor = '';
  /** Bumped after a save: the rows are asked for again (a reorder changes no count), as after an undo or redo. */
  let saved = $state(0);
  $effect(() => {
    const want = id;
    void p?.count;
    void p?.dead;
    void saved;
    void undos.n;
    void listsKey;
    void incomingChanged;
    if (!want || multi) return;
    const c = crateOf(want);
    const a = albumOf(want);
    // As the toggle shows it (the server's saved setting until the crates are known).
    const shownFav = () => (c && crates.list ? favouritesOf(c) : undefined);
    const fav = shownFav();
    const key = `${want}\n${fav}`;
    if (key !== shownFor) {
      rows = null;
      error = null;
      albumHead = null;
      listGenres = null;
    }
    // A crate keeps the pages it has (at least the first) when it is asked for again.
    const asked = c
      ? crateRows(c, 0, Math.max(PAGE, untrack(() => rows?.length ?? 0)), prefs.source || undefined, fav)
      : a
        ? albumView(a.source, a.id).then((r) => {
            if (want === id) albumHead = { title: r.title, artist: r.artist, year: r.year };
            return { ...r, total: r.rows.length };
          })
        : playlistRows(want, prefs.source || undefined).then((r) => ({ ...r, total: r.rows.length, genres: null }));
    asked.then(
      (r) => {
        if (want !== id || shownFav() !== fav) return;
        shownFor = key;
        error = null;
        rows = r.rows;
        source = r.source;
        total = r.total;
        listGenres = r.genres;
        loadFavorites(r.source, r.rows.flatMap((x) => (x.id ? [x.id] : [])));
      },
      (e) => {
        if (want === id) error = e instanceof Error ? e.message : 'Unexpected error';
      },
    );
  });

  // The covers: the playlist's first albums, four as a mosaic (decision 0021).
  let covers = $state<string[]>([]);
  $effect(() => {
    const want = id;
    covers = [];
    if (!want || crateOf(want)) return;
    const a = albumOf(want);
    if (a) {
      covers = [albumCoverUrl(a.source, a.id)];
      return;
    }
    if (multi) {
      // Several: the first cover of each of the first four, as a mosaic.
      const first = ids.slice(0, 4);
      Promise.all(first.map((x) => playlistCovers(x).catch(() => []))).then((lists) => {
        if (want !== id) return;
        const urls = lists.flatMap((l) => (l[0] ? [albumCoverUrl(l[0].source, l[0].album_id)] : []));
        covers = urls.length >= 4 ? urls.slice(0, 4) : urls.slice(0, 1);
      });
      return;
    }
    playlistCovers(want).then(
      (list) => {
        if (want !== id) return;
        const urls = list.map((c) => albumCoverUrl(c.source, c.album_id));
        covers = urls.length >= 4 ? urls.slice(0, 4) : urls.slice(0, 1);
      },
      () => {},
    );
  });

  const length = $derived(rows ? rows.reduce((a, r) => a + (r.duration_secs ?? 0), 0) : 0);
  const playable = $derived(rows ? rows.filter((r) => r.id && !r.missing) : []);
  // An album's tracks carry its id: its cover, and the Incoming page's mark on what plays.
  const tracksOf = (list: PlaylistRow[], src = source): LocalTrack[] =>
    list.map((x) => ({ source: src, id: x.id!, name: x.name ?? x.file, artist: x.artist, album: x.album, album_id: album?.id ?? null }));

  // Several playlists: each section's rows, asked for when it comes into view; a section folds.
  let sections = $state<Record<string, { rows: PlaylistRow[]; source: string } | { error: string }>>({});
  let folded = $state<Record<string, boolean>>({});
  // A playlist left out of the selection drops its section; the others keep theirs.
  const sectionsAsked = new Set<string>();
  $effect(() => {
    const keep = new Set(ids);
    untrack(() => {
      for (const k of Object.keys(sections)) if (!keep.has(k)) delete sections[k];
      for (const k of [...sectionsAsked]) if (!keep.has(k)) sectionsAsked.delete(k);
    });
  });
  function loadSection(sid: string) {
    if (sectionsAsked.has(sid)) return;
    sectionsAsked.add(sid);
    playlistRows(sid, prefs.source || undefined).then(
      (r) => {
        sections[sid] = { rows: r.rows, source: r.source };
        loadFavorites(r.source, r.rows.flatMap((x) => (x.id ? [x.id] : [])));
      },
      (e) => (sections[sid] = { error: e instanceof Error ? e.message : 'Unexpected error' }),
    );
  }
  /** Asks for a section's rows once it is near the screen. */
  function seen(el: HTMLElement, sid: string) {
    const o = new IntersectionObserver(([e]) => e.isIntersecting && loadSection(sid), { rootMargin: '400px' });
    o.observe(el);
    return { destroy: () => o.disconnect() };
  }

  // Related: what Insights knows of it; its genres go in the header, as Finamp's genre line.
  $effect(() => insights.load());
  const at = $derived(insights.indexOf(id));
  const point = $derived(at >= 0 ? insights.shown!.playlists[at] : null);

  $effect(() => void loadPlaylistFavorites());

  // Viewing it again scrolls it to the top.
  let scroller = $state<HTMLElement>();
  $effect(() => {
    void panel.again;
    scroller?.scrollTo({ top: 0, behavior: 'smooth' });
  });

  let menuOpen = $state(false);
  let trackMenu = $state<{ track: MenuTrack; start: 'menu' | 'lists' } | null>(null);
  const asTrack = (r: PlaylistRow, src = source): MenuTrack => ({ source: src, id: r.id!, name: r.name ?? r.file, artist: r.artist, album: r.album, album_id: album?.id ?? null });

  // Finamp's playback actions: a new queue (Play, Shuffle), or the whole playlist next or at the end of the queue.
  const ACTIONS: { mode: PlayMode; icon: IconName; label: string }[] = [
    { mode: 'play', icon: 'play', label: 'Play' },
    { mode: 'shuffle', icon: 'shuffle', label: 'Shuffle' },
    { mode: 'next', icon: 'playNext', label: 'Play next' },
    { mode: 'last', icon: 'playlists', label: 'Add to queue' },
  ];
  // A crate plays and shuffles; in place of Play next and Add to queue, its favourites toggle (decision 0024, amended):
  // its icon and label say what the crate shows now.
  const favState = $derived(crate ? FAVOURITES[favouritesOf(crate)] : null);
  // A crate's next page, when the end of the list comes into view.
  let more = $state<HTMLElement>();
  let loadingMore = false;
  $effect(() => {
    const el = more;
    const c = crate;
    if (!el || !c) return;
    const seen = new IntersectionObserver(async ([e]) => {
      if (!e.isIntersecting || loadingMore || !rows || rows.length >= total) return;
      loadingMore = true;
      try {
        const r = await crateRows(c, rows.length, PAGE, prefs.source || undefined, favouritesOf(c));
        if (crate === c && rows) {
          rows = [...rows, ...r.rows];
          loadFavorites(r.source, r.rows.flatMap((x) => (x.id ? [x.id] : [])));
        }
      } catch (err) {
        toast.fail(err, 'More tracks could not be loaded');
      } finally {
        loadingMore = false;
      }
    });
    seen.observe(el);
    return () => seen.disconnect();
  });

  async function act(mode: PlayMode) {
    navigator.vibrate?.(8);
    try {
      if (plain) await actOnRows(mode);
      else await startPlaylists([...ids], mode);
      if (mode === 'next' || mode === 'last') toast.show({ text: mode === 'next' ? 'Plays next.' : 'Added to the queue.', tone: 'done' });
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    }
  }

  /** A crate's or an album's actions work on the tracks it has shown (a crate's pages so far, newest first). */
  async function actOnRows(mode: PlayMode) {
    const tracks = tracksOf(playable);
    if (mode === 'play' || mode === 'shuffle') {
      const order = mode === 'shuffle' ? [...tracks].sort(() => Math.random() - 0.5) : tracks;
      return playPlaylistFrom(title, source, order, 0);
    }
    if (isLocal()) return enqueue(tracks, mode);
    await queueTracks(
      tracks.map((t) => t.id),
      mode,
      source,
    );
    setTimeout(refresh, 700);
  }

  // Make playlist (the save icon): the crate's tracks now as a new playlist, which then shows here. Its name in progress
  // outlives this view (panelWork: going over the page or back beside it builds the view again).
  type Making = { name: string; saving: boolean };
  let making = $state<Making | null>(work.making && work.making.for === untrack(() => panel.viewed) ? (work.making.state as Making) : null);
  $effect(() => {
    const m = making;
    work.making = m ? { for: untrack(() => panel.viewed) ?? '', state: m } : null;
  });
  async function makePlaylist() {
    if (!making || !crate || !making.name.trim()) return;
    making.saving = true;
    const name = making.name.trim();
    try {
      await favouritesSaved();
      const { added, undo } = await crateToPlaylist(crate, name, prefs.source || undefined);
      withUndo(`Made ${name}: ${added} ${added === 1 ? 'track' : 'tracks'}.`, [undo], {
        after: (undone) => (undone ? followDelete(name) : followCreate(name)),
      });
      making = null;
      refresh();
      loadCrates();
      viewPlaylist(name);
    } catch (e) {
      toast.fail(e, "The playlist wasn't made");
      if (making) making.saving = false;
    }
  }

  /** A tap on a row: its playlist (`name`, these rows) plays from it. */
  async function playFrom(name: string, src: string, list: PlaylistRow[], r: PlaylistRow) {
    if (!r.id || r.missing) return;
    navigator.vibrate?.(8);
    const ok = list.filter((x) => x.id && !x.missing);
    try {
      await playPlaylistFrom(name, src, tracksOf(ok, src), ok.indexOf(r));
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    }
  }

  // A swipe left plays the track next (Finamp's default swipe on a playlist's rows); the row springs back.
  function playNext(r: PlaylistRow, src = source) {
    (async () => {
      try {
        if (isLocal()) enqueue(tracksOf([r], src), 'next');
        else await queueTrack(r.id!, 'next', src);
        toast.show({ text: 'Plays next.', tone: 'done' });
        setTimeout(refresh, 700);
      } catch (e) {
        toast.fail(e, "It couldn't be put next");
      }
    })();
    return 'back' as const;
  }

  // Edit mode: a working copy of the rows (each with its place in the file, so a restored one goes back there), and of
  // the playlist's name: its title is editable where it is, looking the same (a tap puts the caret in it; Enter or a
  // tap elsewhere ends it), and Save renames it with the rest.
  // The edit under way outlives this view (panelWork), as Make playlist's name does.
  type Item = { r: PlaylistRow; at: number };
  type Editing = { rows: Item[]; removed: Item[]; name: string; saving: boolean };
  let editing = $state<Editing | null>(work.editing && work.editing.for === untrack(() => panel.viewed) ? (work.editing.state as Editing) : null);
  $effect(() => {
    const e = editing;
    work.editing = e ? { for: untrack(() => panel.viewed) ?? '', state: e } : null;
  });
  const editingFor = $derived(editing ? id : null);
  $effect(() => {
    // Another playlist viewed (or this one closed): the edit is dropped.
    if (editingFor !== null && editingFor !== id) editing = null;
  });
  const tracksChanged = $derived(!!editing && (editing.removed.length > 0 || editing.rows.some((x, i) => x.at !== i)));
  const newName = $derived(editing ? editing.name.trim() : '');
  const nameChanged = $derived(!!editing && newName !== '' && newName !== id);
  // The pencil beside the name in edit mode: the caret goes to the name's end, as a tap on the name puts it there.
  let nameEl = $state<HTMLElement>();
  function editName() {
    const el = nameEl;
    if (!el) return;
    el.focus();
    const range = document.createRange();
    range.selectNodeContents(el);
    range.collapse(false);
    const sel = getSelection();
    sel?.removeAllRanges();
    sel?.addRange(range);
  }
  function startEdit() {
    if (!rows) return;
    navigator.vibrate?.(8);
    editing = { rows: rows.map((r, at) => ({ r, at })), removed: [], name: id, saving: false };
  }
  const reorder = new Reorder(
    () => scroller,
    () => editing?.rows.length ?? 0,
    (from, to) => {
      if (!editing) return;
      const list = [...editing.rows];
      const [x] = list.splice(from, 1);
      list.splice(to, 0, x);
      editing.rows = list;
    },
  );
  function removeAt(i: number) {
    if (!editing) return;
    const [x] = editing.rows.splice(i, 1);
    editing.removed = [...editing.removed, x];
  }
  function restore(x: Item) {
    if (!editing) return;
    editing.removed = editing.removed.filter((y) => y !== x);
    const before = editing.rows.findIndex((y) => y.at > x.at);
    editing.rows.splice(before < 0 ? editing.rows.length : before, 0, x);
  }
  const removeSwiped = (i: number) => {
    setTimeout(() => removeAt(i), 200);
    return 'away' as const;
  };
  async function save() {
    if (!editing || !rows) return;
    if (!tracksChanged && !nameChanged) return void (editing = null);
    editing.saving = true;
    // Fixed now: once renamed, the panel follows the new name, and what changed would no longer show.
    const [from, to, renamed] = [id, newName, nameChanged];
    const tokens: (string | null)[] = [];
    try {
      if (tracksChanged)
        tokens.push(
          (
            await editPlaylist(
              id,
              rows.map((r) => r.entry),
              editing.rows.map((x) => x.r.entry),
            )
          ).undo,
        );
      // Last: the writes above still name it as it was. The panel's view follows the new name.
      if (renamed) tokens.push(await renameEverywhere(from, to));
      // One Undo for both (the rename undone first, then the tracks under the old name).
      withUndo(`Saved ${to}.`, tokens, {
        after: (undone) => renamed && (undone ? followRename(to, from) : followRename(from, to)),
      });
      editing = null;
      saved++;
      refresh();
    } catch (e) {
      toast.fail(e, "The playlist wasn't saved");
      if (editing) editing.saving = false;
    }
  }

  // Esc closes it (when no sheet of its own is open; in edit mode Cancel is the way out).
  $effect(() => {
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && !menuOpen && !trackMenu && !editing && !making && closePlaylist();
    addEventListener('keydown', esc);
    return () => removeEventListener('keydown', esc);
  });
</script>

<section class="view" aria-label={title}>
  <div class="bar" class:editing={!!editing}>
    {#if editing}
      <!-- ✕ stays in its corner while editing (closing drops the edit), as the place button in the other; Cancel sits
           against Save. -->
      <button class="icon" aria-label="Close the playlist" onclick={() => closePlaylist()}><Icon name="close" size={24} /></button>
      <span class="end">
        <button class="barbtn iconic" class:destructive={tracksChanged || nameChanged} aria-label="Cancel" onclick={() => (editing = null)} disabled={editing.saving}><Icon name="reset" size={18} /><span>Cancel</span></button>
        <button class="barbtn save iconic" aria-label="Save" onclick={save} disabled={editing.saving} aria-busy={editing.saving}><Icon name="arrowDownToLine" size={18} /><span>Save</span></button>
        <PanelPlace />
      </span>
    {:else if making}
      <button class="icon" aria-label="Close the crate" onclick={() => closePlaylist()}><Icon name="close" size={24} /></button>
      <span class="end">
        <button class="barbtn iconic" aria-label="Cancel" onclick={() => (making = null)} disabled={making.saving}><Icon name="reset" size={18} /><span>Cancel</span></button>
        <button class="barbtn save" onclick={makePlaylist} disabled={making.saving || !making.name.trim()} aria-busy={making.saving}>Make playlist</button>
        <PanelPlace />
      </span>
    {:else if crate}
      <button class="icon" aria-label="Close the crate" onclick={() => closePlaylist()}><Icon name="close" size={24} /></button>
      <span class="end">
        {#if !isPermanentCrate}<button class="icon" aria-label={`Change ${title}`} onclick={() => viewPlaylist(EDIT_CRATE + crate)}><Icon name="pencil" size={22} /></button>{/if}
        <button class="icon" aria-label="Make playlist" disabled={!rows?.length} onclick={() => (making = { name: '', saving: false })}><Icon name="intoBox" size={22} /></button>
        <PanelPlace />
      </span>
    {:else if album}
      <button class="icon" aria-label="Close the album" onclick={() => closePlaylist()}><Icon name="close" size={24} /></button>
      <span class="end"><PanelPlace /></span>
    {:else}
      <button class="icon" aria-label={multi ? 'Close the playlists' : 'Close the playlist'} onclick={() => closePlaylist()}><Icon name="close" size={24} /></button>
      <span class="end">
        {#if p && tab === 'tracks'}<button class="icon" aria-label={`Edit ${id}`} disabled={!rows} onclick={startEdit}><Icon name="pencil" size={22} /></button>{/if}
        <button class="icon" class:fav={allPinned} aria-pressed={allPinned} aria-label={allPinned ? 'Unpin' : 'Pin to top'} title={allPinned ? 'Unpin' : 'Pin to top'} onclick={togglePins}
          ><Icon name="pin" size={22} /></button
        >
        <button class="icon" class:fav={allFav} aria-pressed={allFav} aria-label="Favourite" onclick={() => setPlaylistFavorites([...ids], !allFav)}
          ><Icon name="heart" size={24} /></button
        >
        <button class="icon" aria-label={`Menu of ${title}`} onclick={() => (menuOpen = true)}><Icon name="moreVert" size={24} /></button>
        <PanelPlace />
      </span>
    {/if}
  </div>

  <div class="scroll" bind:this={scroller}>
    <header class="head">
      <div class="top">
        <div class="art" class:mosaic={covers.length === 4}>
          {#each covers as c, i (i)}<img src={c} alt="" />{:else}<span class="noart"><Icon name={crate ? 'crates' : 'albums'} size={48} /></span>{/each}
        </div>
        {#if making}
          <div class="info fields">
            <label class="field">
              <span>Name of the new playlist</span>
              <!-- svelte-ignore a11y_autofocus -->
              <input type="text" maxlength="100" autofocus bind:value={making.name} disabled={making.saving} onkeydown={(e) => e.key === 'Enter' && makePlaylist()} />
            </label>
          </div>
        {:else}
        <div class="info">
          {#if editing}
            <!-- The name, editable where it is and looking the same; the pencil beside it says so (and puts the caret
                 in it too). -->
            <div class="name-row">
            <h2
              bind:this={nameEl}
              class="editable"
              contenteditable="plaintext-only"
              class:saving={editing.saving}
              aria-label="Name"
              spellcheck="false"
              bind:textContent={editing.name}
              onkeydown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  e.currentTarget.blur();
                }
              }}
              oninput={(e) => {
                const el = e.currentTarget;
                if ((el.textContent ?? '').length > 100) el.textContent = (el.textContent ?? '').slice(0, 100);
              }}
            ></h2>
            <button class="name-pen" aria-label="Rename" title="Rename" disabled={editing.saving} onclick={editName}><Icon name="pencil" size={16} /></button>
            </div>
          {:else}
            <h2>{title}</h2>
          {/if}
          {#if crate}
            <p class="line"><Icon name="music" size={20} /><span>{rows ? plural(total, 'track') : 'Loading…'}</span></p>
          {:else if multi}
            <p class="line"><Icon name="music" size={20} /><span>{plural(chosenCount, 'track')}</span></p>
            <p class="line"><Icon name="playlists" size={20} /><span>{ids.join(', ')}</span></p>
          {:else}
            {#if album && albumHead?.artist}
              <p class="line"><Icon name="user" size={20} /><span>{albumHead.artist}{#if albumHead.year}&nbsp;({albumHead.year}){/if}</span></p>
            {/if}
            <p class="line"><Icon name="music" size={20} /><span>{p ? plural(p.count, 'track') : rows ? plural(rows.length, 'track') : 'Loading…'}{#if length}&nbsp;({clockText(length)}){/if}</span></p>
          {/if}
          {#if point && !multi}
            <p class="line"><Icon name="swatch" size={20} />{#if point.genres.length}<span class="genres">{#each point.genres as g (g)}<span>{g}</span>{/each}</span>{:else}<span>No genres</span>{/if}</p>
          {:else if (album || crate) && listGenres?.length}
            <p class="line"><Icon name="swatch" size={20} /><span class="genres">{#each listGenres as g (g)}<span>{g}</span>{/each}</span></p>
          {/if}
          {#if p?.dead || chosenDead}<p class="line dead"><span>{p?.dead || chosenDead} missing</span></p>{/if}
        </div>
        {/if}
      </div>
      {#if !editing && !making}
      <div class="actions" role="group" aria-label="Play">
        {#each crate ? ACTIONS.slice(0, 2) : ACTIONS as a (a.mode)}
          <button onclick={() => act(a.mode)} disabled={multi ? !chosenCount : !playable.length}><Icon name={a.icon} size={28} /><span>{a.label}</span></button>
        {/each}
        {#if crate && favState}
          <button class="favs" class:filled={favState.filled} aria-label={`Showing: ${favState.label}. Tap for the next`} onclick={() => crate && cycleFavourites(crate)}
            ><Icon name={favState.icon} size={28} /><span>{favState.label}</span></button
          >
        {/if}
      </div>
      {/if}
    </header>

    {#if !plain && !editing}
      <div class="tabs" role="tablist" aria-label="About the playlist">
        {#each TABS as t (t.tab)}
          <button role="tab" aria-selected={tab === t.tab} onclick={() => setTab(t.tab, !inSidebar())}>{t.label}</button>
        {/each}
      </div>
    {/if}

    {#if tab === 'info'}
      <div class="info-tab">
        {#each ids as pid (pid)}
          {#if multi}
            <Section title={pid}>{@render about(pid)}</Section>
          {:else}
            {@render about(pid)}
          {/if}
        {/each}
      </div>
    {:else if tab === 'export'}
      <ExportTab {ids} />
    {:else if tab === 'repair'}
      <RepairTab {ids} />
    {:else if tab === 'sync'}
      <SyncTab {ids} />
    {:else if multi}
      {#each ids as sid (sid)}
        {@const s = sections[sid]}
        {@const count = shownPlaylists().find((x) => x.id === sid)?.count ?? 0}
        <Section title={sid} count={plural(count, 'track')} bind:open={() => !folded[sid], (v) => (folded[sid] = !v)} attach={fromAction(seen, () => sid)}>
          {#if s && 'error' in s}
            <p class="error" role="alert">{s.error}</p>
          {:else if !s}
            <p class="empty">Loading…</p>
          {:else if !s.rows.length}
            <p class="empty">No tracks yet.</p>
          {:else}
            <ol class="rows">
              {#each s.rows as r, i (i)}{@render trackRow(r, s.source, sid, s.rows)}{/each}
            </ol>
          {/if}
        </Section>
      {/each}
    {:else if editing}
      <ol class="rows edit" class:dragging={!!reorder.drag}>
        {#each editing.rows as x, i (x.at)}
          {@const r = x.r}
          <li class:lifted={reorder.drag?.from === i} style:transform={reorder.drag ? `translateY(${reorder.offset(i)}px)` : null}>
            <Row
              size="large"
              title={r.name ?? r.file}
              sub={r.missing ? 'Missing file' : !r.id ? 'Not matched yet' : (r.artist ?? '')}
              off={!r.id || r.missing}
              attach={fromAction(swipe, () => ({ enabled: !reorder.drag && !editing!.saving, onswipe: () => removeSwiped(i) }))}
            >
              {#snippet lead()}{@render cover(r, source)}{/snippet}
              {#snippet action()}
                <button class="tool" aria-label={`Remove ${r.name ?? r.file}`} onclick={() => removeAt(i)}><Icon name="close" size={22} /></button>
                <button
                  class="tool grip"
                  aria-label="Drag to reorder"
                  onpointerdown={(e) => reorder.down(e, i)}
                  onpointermove={reorder.move}
                  onpointerup={reorder.end}
                  onpointercancel={reorder.end}><Icon name="order" size={24} /></button
                >
              {/snippet}
            </Row>
          </li>
        {/each}
      </ol>
      {#if editing.removed.length}
        <h3 class="removed-head">Removed tracks</h3>
        <ol class="rows">
          {#each editing.removed as x (x.at)}
            <li>
              <Row size="large" title={x.r.name ?? x.r.file} sub={x.r.artist ?? ''} off>
                {#snippet lead()}{@render cover(x.r, source)}{/snippet}
                {#snippet action()}<button class="tool" aria-label={`Restore ${x.r.name ?? x.r.file}`} onclick={() => restore(x)}><Icon name="back" size={22} /></button>{/snippet}
              </Row>
            </li>
          {/each}
        </ol>
      {/if}
    {/if}

    {#if tab !== 'tracks' || multi || editing}
      <!-- another tab, the sections, or the edit list: above -->
    {:else if error}
      <p class="error" role="alert">{error}</p>
    {:else if rows && !rows.length}
      <p class="empty">No tracks yet.</p>
    {:else if rows}
      <ol class="rows">
        {#each rows as r, i (i)}{@render trackRow(r, source, title, rows)}{/each}
      </ol>
      {#if crate && rows.length < total}<p class="empty" bind:this={more}>Loading…</p>{/if}
    {:else}
      <p class="empty">Loading…</p>
    {/if}
  </div>
</section>

<!-- The Info tab's part for one playlist: how focused it is, and its closest playlists as large rows (a tap views one). -->
{#snippet about(pid: string)}
  {@const i = insights.indexOf(pid)}
  {@const pt = i >= 0 ? insights.shown!.playlists[i] : null}
  {@const near = insights.closest(i)}
  <div class="about" aria-label={`About ${pid}`}>
    {#if !pt}
      <p class="note">{insights.shown ? 'Nothing is known about it yet.' : 'Loading…'}</p>
    {:else}
      {#if pt.focus !== null && insights.focus[i] !== undefined}
        <p class="note">More focused than {Math.round(100 * insights.focus[i])}% of your playlists</p>
      {/if}
      {#if near.length}
        <h3 class="list-head">Closest playlists</h3>
        <ul class="plain">
          {#each near as c (c.index)}
            <li>
              <PlaylistItem
                p={shownPlaylists().find((x) => x.id === c.name) ?? { id: c.name, name: c.name }}
                size="large"
                sub={c.shares}
                label={`View ${c.name}`}
                onclick={() => viewPlaylist(c.name)}
              />
            </li>
          {/each}
        </ul>
      {:else if pt.focus === null}
        <p class="note">Nothing is known about it yet.</p>
      {/if}
    {/if}
  </div>
{/snippet}

<!-- A track's row: a tap plays its playlist (`name`, these rows) from it, a swipe left plays it next, a long-press is its
     menu, the heart adds it to playlists. -->
{#snippet trackRow(r: PlaylistRow, src: string, name: string, list: PlaylistRow[])}
  {@const ok = !!r.id && !r.missing}
  {@const here = ok && !!np && np.id === r.id}
  <li class:now={here}>
    <span class="behind" aria-hidden="true"><Icon name="playNext" size={40} /><span>Play next</span></span>
    <Row
      size="large"
      title={r.name ?? r.file}
      count={r.duration_secs ? clockText(r.duration_secs) : undefined}
      off={!ok}
      disabled={!ok}
      onclick={() => playFrom(name, src, list, r)}
      attach={fromAction(swipe, () => ({ enabled: ok, onswipe: () => playNext(r, src) }))}
      attachMain={fromAction(longpress, () => () => ok && (trackMenu = { track: asTrack(r, src), start: 'menu' }))}
    >
      {#snippet lead()}{@render cover(r, src, here)}{/snippet}
      {#snippet sub()}
        {#if r.missing}Missing file{:else if !r.id}Not matched yet{:else}<span class="artist">{r.artist ?? ''}</span>{#if r.album}<span class="album">{r.album}</span>{/if}{/if}
      {/snippet}
      {#snippet action()}
        {#if ok}<RowHeart source={src} id={r.id!} name={r.name ?? r.file} onopen={() => (trackMenu = { track: asTrack(r, src), start: 'lists' })} />{/if}
      {/snippet}
    </Row>
  </li>
{/snippet}

{#snippet cover(r: PlaylistRow, src: string, here = false)}
  <!-- An Incoming album's tracks are outside Music, where the track covers come from: the album's own. -->
  <TrackCover src={album ? albumCoverUrl(album.source, album.id) : r.id && !r.missing ? coverUrl({ source: src, id: r.id }) : null} playing={here} paused={paused()} />
{/snippet}

{#if menuOpen && selected.length}
  <PlaylistMenu playlists={selected} onclose={() => (menuOpen = false)} />
{/if}
{#if trackMenu}
  <TrackMenu track={trackMenu.track} start={trackMenu.start} onclose={() => (trackMenu = null)} />
{/if}

<style>
  .view {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    /* As the navigation beside the page. */
    background: var(--surface);
  }

  /* Finamp's app bar: no title (the name is in the header), back on the left, the menu on the right. */
  .bar {
    flex: none;
    display: flex;
    justify-content: space-between;
    padding: 4px 4px 0;
  }

  .end {
    display: flex;
    align-items: center;
  }

  /* Finamp's playlist heart: filled when it is a favourite; the pin filled when pinned. */
  .icon.fav :global(svg) {
    fill: currentColor;
  }

  /* The narrowest panel: the bar's icons a little closer, so ✕, the pencil, the pin, the heart, the menu and the
     place button fit on one line. */
  @container (max-width: 299px) {
    .bar .icon,
    .bar :global(.place) {
      width: 42px;
    }
  }

  /* Edit mode's Cancel and Save, where ✕ and the menu are otherwise. */
  .barbtn {
    min-height: 40px;
    margin: 4px 8px;
    padding: 0 14px;
    border: 0;
    border-radius: 10px;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  /* Cancel and Save carry an icon beside their label (a reset arrow, an arrow down to a line). */
  .barbtn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  /* A narrow panel: Cancel and Save show their icons only (the labels stay for screen readers). */
  @container (max-width: 339px) {
    .barbtn.iconic span {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip-path: inset(50%);
      white-space: nowrap;
    }

    .barbtn.iconic {
      padding: 0 10px;
    }
  }

  /* Cancel with changes made: it throws them away, so it reads as destructive. */
  .barbtn.destructive {
    color: var(--danger, #c33);
  }

  .barbtn.save {
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  .barbtn:disabled {
    opacity: 0.6;
  }

  .tool {
    flex: none;
    display: grid;
    place-items: center;
    width: 40px;
    height: 100%;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  .grip {
    cursor: grab;
    touch-action: none;
  }

  .rows.edit.dragging li {
    transition: transform 200ms ease;
  }

  .rows.edit.dragging li.lifted {
    z-index: 1;
    transition: none;
  }

  /* The name in edit mode: the title as it is, with a caret once tapped (no field around it); all of it shows while
     it is being written. */
  h2.editable {
    cursor: text;
    outline: none;
  }

  .name-row {
    display: flex;
    align-items: flex-start;
    min-width: 0;
  }

  .name-row h2 {
    flex: 0 1 auto;
    min-width: 0;
  }

  .name-pen {
    flex: none;
    display: grid;
    place-items: center;
    width: 28px;
    height: 24px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    cursor: pointer;
  }

  /* While it saves the name is not written in. */
  h2.editable.saving {
    pointer-events: none;
  }

  h2.editable:focus {
    display: block;
    overflow: visible;
  }

  /* Make playlist's name field, in place of the info beside the cover. */
  .info.fields {
    justify-content: center;
    gap: 8px;
  }

  .field {
    display: grid;
    gap: 4px;
    padding: 0 6px;
    font-size: 13px;
    color: var(--muted);
  }

  .field input {
    min-height: 40px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    color: var(--text);
    font: inherit;
    font-size: 15px;
  }

  .removed-head {
    margin: 16px 16px 4px;
    font-size: 14px;
    font-weight: 500;
  }

  .icon {
    display: grid;
    place-items: center;
    width: 48px;
    height: 48px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  /* The pencil while the tracks load: in its place, not yet usable. */
  .icon:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    /* On a phone the mini-player floats over the end of the list (as Finamp's now-playing bar): room to scroll past it. */
    padding-bottom: calc(32px + var(--mini-h, 0px) + env(safe-area-inset-bottom));
  }

  /* Never wider than the panel (as the tabs and their content below): its one column is the panel's width at most,
     not what its widest part would like. */
  .head {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 10px;
    padding: 0 8px 8px;
  }

  /* The cover beside the info, as Finamp's header: 125 px, corners 4. */
  .top {
    display: flex;
    gap: 4px;
  }

  .art {
    flex: none;
    width: 125px;
    height: 125px;
    overflow: hidden;
    border-radius: 4px;
    background: var(--bg);
  }

  .art.mosaic {
    display: grid;
    grid-template-columns: 1fr 1fr;
  }

  .art img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .noart {
    display: grid;
    place-items: center;
    width: 100%;
    height: 100%;
    color: var(--muted);
  }

  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    padding-bottom: 2px;
  }

  h2 {
    display: -webkit-box;
    overflow: hidden;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    margin: 0;
    padding: 0 6px 6px;
    font-size: 17px;
    font-weight: 500;
    line-height: 1.25;
  }

  .line {
    display: flex;
    align-items: center;
    gap: 4px;
    margin: 0;
    padding: 2px 6px;
    font-size: 14px;
    min-width: 0;
  }

  .line > :global(svg) {
    flex: none;
    opacity: 0.5;
  }

  .line > span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .genres {
    display: flex;
    gap: 4px;
    overflow: hidden;
  }

  .genres span {
    flex: none;
    padding: 1px 8px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--text) 10%, transparent);
    font-size: 12px;
  }

  .dead {
    color: var(--danger, #c33);
  }

  /* Finamp's playback actions: unfilled, an icon over a small label, spread evenly. */
  .actions {
    display: flex;
    justify-content: space-evenly;
  }

  .actions button {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    padding: 5px 4px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  .actions button:hover:not(:disabled) {
    background: color-mix(in srgb, var(--text) 7%, transparent);
  }

  .actions button span {
    font-size: 12px;
    font-weight: 300;
    line-height: 1.4;
  }

  /* The crate's favourites toggle: Only favourites fills its heart. */
  .actions .favs.filled :global(svg) {
    fill: currentColor;
  }

  /* A narrow panel: the actions show their icons only (their names stay as their labels for screen readers). */
  @container (max-width: 339px) {
    .actions button span {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip-path: inset(50%);
      white-space: nowrap;
    }
  }

  .actions button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .info-tab {
    padding: 0 8px;
  }

  .about {
    padding: 4px 0 8px;
  }

  .note {
    margin: 4px 8px;
    color: var(--muted);
    font-size: 13px;
  }

  /* A list's heading inside a tab (Closest playlists): small, over its large rows. */
  .list-head {
    margin: 12px 8px 4px;
    color: var(--muted);
    font-size: 13px;
    font-weight: 500;
  }

  ul.plain {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* The tabs (decision 0025): under the header, staying at the top while the content scrolls. */
  .tabs {
    position: sticky;
    top: 0;
    z-index: 2;
    display: flex;
    gap: 4px;
    margin-bottom: 8px;
    padding: 4px 8px 0;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }

  .tabs button {
    flex: 1;
    min-height: 40px;
    padding: 0 8px;
    border: 0;
    border-bottom: 2px solid transparent;
    background: none;
    color: var(--muted);
    font: inherit;
    font-size: 14px;
    cursor: pointer;
  }

  .tabs button[aria-selected='true'] {
    border-bottom-color: var(--accent);
    color: var(--text);
    font-weight: 600;
  }

  /* Several playlists on Tracks: a section each, its header folding it. */
  /* Finamp's track rows: 70 px (a 60 px tile and a gap), a 60 px cover with corners 8, two lines of title. */
  .rows {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .rows li {
    position: relative;
    height: 70px;
    padding: 6px 8px 0;
    box-sizing: border-box;
    /* Only rows on screen are drawn (a 500-track playlist shows fast). */
    content-visibility: auto;
    contain-intrinsic-size: auto 70px;
    /* The row is opaque over what its swipe shows. */
    --row-bg: var(--surface);
  }

  /* The playing track: tinted in the cover's colour (Finamp: primary at 30–35 %). */
  .rows li.now {
    --row-bg: color-mix(in srgb, var(--art-accent, var(--accent)) 32%, var(--surface));
  }

  /* Finamp's track line under the title: the artist, then the album lighter. */
  .rows :global(.row .sub) {
    color: inherit;
  }

  /* Behind the row while it is swiped: what the swipe does. */
  .behind {
    position: absolute;
    inset: 6px 20px 0;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 4px;
    color: var(--art-accent, var(--accent));
    font-weight: 500;
    font-size: 14px;
  }

  .artist {
    opacity: 0.75;
  }

  .album {
    margin-left: 10px;
    font-weight: 300;
    opacity: 0.6;
  }

  .empty,
  .error {
    margin: 16px;
    color: var(--muted);
  }

  .error {
    color: var(--danger, #c33);
  }
</style>
