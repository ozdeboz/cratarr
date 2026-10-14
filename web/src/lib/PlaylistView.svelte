<script lang="ts">
  import { untrack } from 'svelte';
  // An opened playlist (decision 0023), in the panel beside the page or in its own layer, styled as Finamp's playlist
  // screen (lib/components/AlbumScreen) in less room: a bar with ✕ and the menu; the covers beside the name, "N tracks
  // (length)" and the genres; Play, Shuffle, Play next and Add to queue as icons over labels; what Insights knows of it
  // (Related, a section until it gets a tab); then the tracks in file order. A tap on a track plays the playlist from
  // there on the chosen output, a swipe left plays it next, a long-press is its menu; the heart adds it to playlists
  // (long-press: favourite). Lines the server does not know, or files that are missing, show by file name, dimmed.
  // Edit mode (the pencil; Finamp's playlist edit screen): rows get the reorder handle and ✕ (a swipe left removes too),
  // removed tracks wait in their own section to be restored, and Save writes the playlist once (Cancel drops it).
  // A crate (decision 0024, `?open=crate:…`) shows the same way, without the statistics (genres, length, Related) and
  // without editing; its rows come in pages (newest first), and the save icon makes a playlist of it (a snapshot).
  // Playlists show in tabs (decision 0025): Tracks, Export, Repair, Sync, under the header, for the selection: one
  // playlist as above, or several (the Playlists page's Select): their covers and count in the header, and on Tracks a
  // section per playlist, in the order ticked, each folding, read-only. A crate has no tabs.
  import {
    coverUrl,
    crateRows,
    crateToPlaylist,
    editPlaylist,
    incomingCoverUrl,
    playlistCovers,
    playlistRows,
    queueTrack,
    queueTracks,
    setSpoken,
    spokenNames,
    type PlaylistRow,
  } from './api';
  import { crateName, crateOf, crates, EDIT_CRATE, loadCrates } from './crates.svelte';
  import { Reorder } from './reorder.svelte';
  import { renameEverywhere } from './rename.svelte';
  import { isPlaylistFavorite, loadFavorites, loadPlaylistFavorites, setPlaylistFavorites } from './favourites.svelte';
  import Icon, { type IconName } from './Icon.svelte';
  import { insights, plural } from './insights.svelte';
  import { refresh } from './live.svelte';
  import { enqueue, type LocalTrack } from './localPlayer.svelte';
  import { longpress } from './longpress';
  import { isLocal, playPlaylistFrom, setPlayingMember, shownPlaylists, startPlaylists } from './output.svelte';
  import { closePlaylist, openPlaylist, panel, setTab, TABS } from './panel.svelte';
  import { inSidebar } from './live.svelte';
  import ExportTab from './ExportTab.svelte';
  import RepairTab from './RepairTab.svelte';
  import SyncTab from './SyncTab.svelte';
  import PlaylistMenu from './PlaylistMenu.svelte';
  import { prefs } from './prefs.svelte';
  import { clockText, paused, playing } from './remote.svelte';
  import RowHeart from './RowHeart.svelte';
  import { swipe } from './swipe';
  import { toast } from './toast.svelte';
  import TrackMenu, { type MenuTrack } from './TrackMenu.svelte';
  import type { PlayMode } from './api';

  const id = $derived(panel.open ?? '');
  /** The open crate's id, when a crate is open (not a playlist). */
  const crate = $derived(crateOf(panel.open));
  /** The selected playlists; several: Tracks shows a section per playlist (decision 0025). */
  const ids = $derived(panel.selection);
  const multi = $derived(ids.length > 1);
  const tab = $derived(crate ? 'tracks' : panel.tab);
  const title = $derived(crate ? crateName(crate) : multi ? `${ids.length} playlists` : id);
  const isPermanentCrate = $derived(!!crate && (crates.list?.find((c) => c.id === crate)?.permanent ?? true));
  const p = $derived(crate || multi ? null : (shownPlaylists().find((x) => x.id === id) ?? null));
  /** The selected playlists, in the order ticked (for the menu, the counts). */
  const selected = $derived(ids.flatMap((x) => shownPlaylists().filter((y) => y.id === x)));
  const chosen = $derived(multi ? selected : []);
  const chosenDead = $derived(chosen.reduce((n, x) => n + x.dead, 0));
  // The heart: on when every selected playlist is a favourite; a tap sets all of them.
  const allFav = $derived(ids.length > 0 && ids.every((x) => isPlaylistFavorite(x)));
  const chosenCount = $derived(chosen.reduce((n, x) => n + x.count, 0));
  const np = $derived(playing());
  /** A crate's size (its rows come in pages). */
  let total = $state(0);
  const PAGE = 200;
  // A crate changes as tracks are sorted into playlists (Unlisted) or hearted: asked for again when the playlists change.
  const listsKey = $derived(crate ? shownPlaylists().map((x) => x.count).join(',') : '');

  // The rows: asked for when the playlist opens and again when it changes (its counts, from the shell's state).
  let rows = $state<PlaylistRow[] | null>(null);
  let source = $state('');
  let error = $state<string | null>(null);
  let shownFor = '';
  /** Bumped after a save: the rows are asked for again (a reorder changes no count). */
  let saved = $state(0);
  $effect(() => {
    const want = id;
    void p?.count;
    void p?.dead;
    void saved;
    void listsKey;
    if (!want || multi) return;
    if (want !== shownFor) {
      rows = null;
      error = null;
    }
    const c = crateOf(want);
    // A crate keeps the pages it has (at least the first) when it is asked for again.
    const asked = c
      ? crateRows(c, 0, Math.max(PAGE, untrack(() => rows?.length ?? 0)), prefs.source || undefined)
      : playlistRows(want, prefs.source || undefined).then((r) => ({ ...r, total: r.rows.length }));
    asked.then(
      (r) => {
        if (want !== id) return;
        shownFor = want;
        rows = r.rows;
        source = r.source;
        total = r.total;
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
    if (multi) {
      // Several: the first cover of each of the first four, as a mosaic.
      const first = ids.slice(0, 4);
      Promise.all(first.map((x) => playlistCovers(x).catch(() => []))).then((lists) => {
        if (want !== id) return;
        const urls = lists.flatMap((l) => (l[0] ? [incomingCoverUrl(l[0].source, l[0].album_id)] : []));
        covers = urls.length >= 4 ? urls.slice(0, 4) : urls.slice(0, 1);
      });
      return;
    }
    playlistCovers(want).then(
      (list) => {
        if (want !== id) return;
        const urls = list.map((c) => incomingCoverUrl(c.source, c.album_id));
        covers = urls.length >= 4 ? urls.slice(0, 4) : urls.slice(0, 1);
      },
      () => {},
    );
  });

  const length = $derived(rows ? rows.reduce((a, r) => a + (r.duration_secs ?? 0), 0) : 0);
  const playable = $derived(rows ? rows.filter((r) => r.id && !r.missing) : []);
  const tracksOf = (list: PlaylistRow[], src = source): LocalTrack[] =>
    list.map((x) => ({ source: src, id: x.id!, name: x.name ?? x.file, artist: x.artist, album: x.album, album_id: null }));

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
  const closest = $derived(insights.closest(at));

  $effect(() => void loadPlaylistFavorites());

  // Opening it again scrolls it to the top.
  let scroller = $state<HTMLElement>();
  $effect(() => {
    void panel.again;
    scroller?.scrollTo({ top: 0, behavior: 'smooth' });
  });

  let menuOpen = $state(false);
  let trackMenu = $state<{ track: MenuTrack; start: 'menu' | 'lists' } | null>(null);
  const asTrack = (r: PlaylistRow, src = source): MenuTrack => ({ source: src, id: r.id!, name: r.name ?? r.file, artist: r.artist, album: r.album, album_id: null });

  // Finamp's playback actions: a new queue (Play, Shuffle), or the whole playlist next or at the end of the queue.
  const ACTIONS: { mode: PlayMode; icon: IconName; label: string }[] = [
    { mode: 'play', icon: 'play', label: 'Play' },
    { mode: 'shuffle', icon: 'shuffle', label: 'Shuffle' },
    { mode: 'next', icon: 'playNext', label: 'Play next' },
    { mode: 'last', icon: 'playlists', label: 'Add to queue' },
  ];
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
        const r = await crateRows(c, rows.length, PAGE, prefs.source || undefined);
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
      if (crate) await actOnCrate(mode);
      else await startPlaylists([...ids], mode);
      if (mode === 'next' || mode === 'last') toast.show({ text: mode === 'next' ? 'Plays next.' : 'Added to the queue.', tone: 'done' });
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    }
  }

  /** A crate's actions work on the tracks it has shown so far (its pages, newest first). */
  async function actOnCrate(mode: PlayMode) {
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

  // Make playlist (the save icon): the crate's tracks now as a new playlist, which then opens here.
  let making = $state<{ name: string; saving: boolean } | null>(null);
  async function makePlaylist() {
    if (!making || !crate || !making.name.trim()) return;
    making.saving = true;
    const name = making.name.trim();
    try {
      await crateToPlaylist(crate, name, prefs.source || undefined);
      making = null;
      refresh();
      loadCrates();
      openPlaylist(name);
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

  async function addPlaying() {
    try {
      await setPlayingMember(id, true);
    } catch (e) {
      toast.fail(e, "It couldn't be added");
    }
  }

  // Edit mode: a working copy of the rows (each with its place in the file, so a restored one goes back there), and of
  // the playlist's name and the names it is also spoken as (decision 0015). "Names" in the bar shows those two in
  // place of the info beside the cover (Finamp's edit screen has its name field there).
  type Item = { r: PlaylistRow; at: number };
  let editing = $state<{ rows: Item[]; removed: Item[]; name: string; spoken: string; names: boolean; saving: boolean } | null>(null);
  const editingFor = $derived(editing ? id : null);
  $effect(() => {
    // Another playlist opened (or this one closed): the edit is dropped.
    if (editingFor !== null && editingFor !== id) editing = null;
  });
  const tracksChanged = $derived(!!editing && (editing.removed.length > 0 || editing.rows.some((x, i) => x.at !== i)));
  const spokenChanged = $derived(!!editing && spokenNames(editing.spoken).join('\n') !== (p?.spoken ?? []).join('\n'));
  const newName = $derived(editing ? editing.name.trim() : '');
  const nameChanged = $derived(!!editing && newName !== '' && newName !== id);
  function startEdit() {
    if (!rows) return;
    navigator.vibrate?.(8);
    editing = { rows: rows.map((r, at) => ({ r, at })), removed: [], name: id, spoken: (p?.spoken ?? []).join(', '), names: false, saving: false };
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
    if (!tracksChanged && !spokenChanged && !nameChanged) return void (editing = null);
    editing.saving = true;
    try {
      if (tracksChanged)
        await editPlaylist(
          id,
          rows.map((r) => r.entry),
          editing.rows.map((x) => x.r.entry),
        );
      if (spokenChanged) await setSpoken(id, spokenNames(editing.spoken));
      // Last: the writes above still name it as it was. The open panel follows the new name.
      if (nameChanged) await renameEverywhere(id, newName);
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

<section class="view" aria-label={id}>
  <div class="bar">
    {#if editing}
      <button class="barbtn" onclick={() => (editing = null)} disabled={editing.saving}>Cancel</button>
      <span class="end">
        <button class="barbtn" aria-pressed={editing.names} onclick={() => editing && (editing.names = !editing.names)} disabled={editing.saving}>Names</button>
        <button class="barbtn save" onclick={save} disabled={editing.saving} aria-busy={editing.saving}>Save</button>
      </span>
    {:else if making}
      <button class="barbtn" onclick={() => (making = null)} disabled={making.saving}>Cancel</button>
      <button class="barbtn save" onclick={makePlaylist} disabled={making.saving || !making.name.trim()} aria-busy={making.saving}>Make playlist</button>
    {:else if crate}
      <button class="icon" aria-label="Close the crate" onclick={() => closePlaylist()}><Icon name="close" size={24} /></button>
      <span class="end">
        {#if !isPermanentCrate}<button class="icon" aria-label={`Change ${title}`} onclick={() => openPlaylist(EDIT_CRATE + crate)}><Icon name="pencil" size={22} /></button>{/if}
        <button class="icon" aria-label="Make playlist" disabled={!rows?.length} onclick={() => (making = { name: '', saving: false })}><Icon name="save" size={22} /></button>
      </span>
    {:else}
      <button class="icon" aria-label={multi ? 'Close the playlists' : 'Close the playlist'} onclick={() => closePlaylist()}><Icon name="close" size={24} /></button>
      <span class="end">
        <button class="icon" class:fav={allFav} aria-pressed={allFav} aria-label="Favourite" onclick={() => setPlaylistFavorites([...ids], !allFav)}
          ><Icon name="heart" size={24} /></button
        >
        {#if rows && p && tab === 'tracks'}<button class="icon" aria-label={`Edit ${id}`} onclick={startEdit}><Icon name="pencil" size={22} /></button>{/if}
        <button class="icon" aria-label={`Menu of ${title}`} onclick={() => (menuOpen = true)}><Icon name="moreVert" size={24} /></button>
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
        {:else if editing?.names}
          <div class="info fields">
            <label class="field">
              <span>Name</span>
              <input type="text" maxlength="100" bind:value={editing.name} disabled={editing.saving} />
            </label>
            <label class="field">
              <span>Spoken as</span>
              <input type="text" placeholder="Other names, comma-separated" bind:value={editing.spoken} disabled={editing.saving} />
            </label>
          </div>
        {:else}
        <div class="info">
          <h2>{title}</h2>
          {#if crate}
            <p class="line"><Icon name="music" size={20} /><span>{rows ? plural(total, 'track') : 'Loading…'}</span></p>
          {:else if multi}
            <p class="line"><Icon name="music" size={20} /><span>{plural(chosenCount, 'track')}</span></p>
            <p class="line"><Icon name="playlists" size={20} /><span>{ids.join(', ')}</span></p>
          {:else}
            <p class="line"><Icon name="music" size={20} /><span>{p ? plural(p.count, 'track') : rows ? plural(rows.length, 'track') : 'Loading…'}{#if length}&nbsp;({clockText(length)}){/if}</span></p>
          {/if}
          {#if point && !multi}
            <p class="line"><Icon name="swatch" size={20} />{#if point.genres.length}<span class="genres">{#each point.genres as g (g)}<span>{g}</span>{/each}</span>{:else}<span>No genres</span>{/if}</p>
          {/if}
          {#if p?.dead || chosenDead}<p class="line dead"><span>{p?.dead || chosenDead} missing</span></p>{/if}
        </div>
        {/if}
      </div>
      {#if !editing && !making && tab === 'tracks'}
      <div class="actions" role="group" aria-label="Play">
        {#each ACTIONS as a (a.mode)}
          <button onclick={() => act(a.mode)} disabled={multi ? !chosenCount : !playable.length}><Icon name={a.icon} size={28} /><span>{a.label}</span></button>
        {/each}
      </div>
      {#if np && np.addable && p && !p.member}
        <button class="add-playing" onclick={addPlaying}><Icon name="plus" size={18} /><span>Add what's playing</span></button>
      {/if}
      {/if}
    </header>

    {#if !crate && !editing}
      <div class="tabs" role="tablist" aria-label="About the playlist">
        {#each TABS as t (t.tab)}
          <button role="tab" aria-selected={tab === t.tab} onclick={() => setTab(t.tab, !inSidebar())}>{t.label}</button>
        {/each}
      </div>
    {/if}

    {#if tab === 'export'}
      <ExportTab {ids} />
    {:else if tab === 'repair'}
      <RepairTab {ids} />
    {:else if tab === 'sync'}
      <SyncTab {ids} />
    {:else if multi}
      {#each ids as sid (sid)}
        {@const s = sections[sid]}
        {@const count = shownPlaylists().find((x) => x.id === sid)?.count ?? 0}
        <section class="sect" use:seen={sid}>
          <button class="sect-head" class:folded={folded[sid]} aria-expanded={!folded[sid]} onclick={() => (folded[sid] = !folded[sid])}
            ><span class="sname">{sid}</span><span class="scount">{plural(count, 'track')}</span><Icon name="chevronUp" size={20} /></button
          >
          {#if !folded[sid]}
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
          {/if}
        </section>
      {/each}
    {:else if editing}
      <ol class="rows edit" class:dragging={!!reorder.drag}>
        {#each editing.rows as x, i (x.at)}
          {@const r = x.r}
          <li class:off={!r.id || r.missing} class:lifted={reorder.drag?.from === i} style:transform={reorder.drag ? `translateY(${reorder.offset(i)}px)` : null}>
            <div class="slide" use:swipe={{ enabled: !reorder.drag && !editing.saving, onswipe: () => removeSwiped(i) }}>
              <span class="row">
                <span class="cover">{#if r.id && !r.missing}<img src={coverUrl({ source, id: r.id })} alt="" loading="lazy" />{:else}<span class="noimg"></span>{/if}</span>
                <span class="text"><strong>{r.name ?? r.file}</strong><span class="sub">{r.missing ? 'Missing file' : !r.id ? 'Not matched yet' : (r.artist ?? '')}</span></span>
              </span>
              <button class="tool" aria-label={`Remove ${r.name ?? r.file}`} onclick={() => removeAt(i)}><Icon name="close" size={22} /></button>
              <button class="tool grip" aria-label="Drag to reorder" onpointerdown={(e) => reorder.down(e, i)} onpointermove={reorder.move} onpointerup={reorder.end} onpointercancel={reorder.end}
                ><Icon name="order" size={24} /></button
              >
            </div>
          </li>
        {/each}
      </ol>
      {#if editing.removed.length}
        <h3 class="removed-head">Removed tracks</h3>
        <ol class="rows">
          {#each editing.removed as x (x.at)}
            <li class="off">
              <div class="slide">
                <span class="row">
                  <span class="cover">{#if x.r.id && !x.r.missing}<img src={coverUrl({ source, id: x.r.id })} alt="" loading="lazy" />{:else}<span class="noimg"></span>{/if}</span>
                  <span class="text"><strong>{x.r.name ?? x.r.file}</strong><span class="sub">{x.r.artist ?? ''}</span></span>
                </span>
                <button class="tool" aria-label={`Restore ${x.r.name ?? x.r.file}`} onclick={() => restore(x)}><Icon name="back" size={22} /></button>
              </div>
            </li>
          {/each}
        </ol>
      {/if}
    {:else if point && (closest.length || point.focus !== null)}
      <section class="related" aria-label="Related">
        {#if point.focus !== null && insights.focus[at] !== undefined}
          <p class="note">More focused than {Math.round(100 * insights.focus[at])}% of your playlists</p>
        {/if}
        {#if closest.length}
          <h3>Closest playlists</h3>
          <ul>
            {#each closest as c (c.index)}
              <li><button class="link" onclick={() => openPlaylist(c.name)}>{c.name}</button><span class="note">{c.shares}</span></li>
            {/each}
          </ul>
        {/if}
      </section>
    {/if}

    {#if tab !== 'tracks' || multi || editing}
      <!-- another tab, the sections, or the edit list: above -->
    {:else if error}
      <p class="error" role="alert">{error}</p>
    {:else if rows && !rows.length}
      <p class="empty">No tracks yet.</p>
    {:else if rows}
      <ol class="rows">
        {#each rows as r, i (i)}{@render trackRow(r, source, id, rows)}{/each}
      </ol>
      {#if crate && rows.length < total}<p class="empty" bind:this={more}>Loading…</p>{/if}
    {:else}
      <p class="empty">Loading…</p>
    {/if}
  </div>
</section>

<!-- A track's row: a tap plays its playlist (`name`, these rows) from it, a swipe left plays it next, a long-press is its
     menu, the heart adds it to playlists. -->
{#snippet trackRow(r: PlaylistRow, src: string, name: string, list: PlaylistRow[])}
  {@const ok = !!r.id && !r.missing}
  {@const here = ok && !!np && np.id === r.id}
  <li class:off={!ok} class:now={here}>
    <span class="behind" aria-hidden="true"><Icon name="playNext" size={40} /><span>Play next</span></span>
    <div class="slide" use:swipe={{ enabled: ok, onswipe: () => playNext(r, src) }}>
      <button class="row" disabled={!ok} onclick={() => playFrom(name, src, list, r)} use:longpress={() => ok && (trackMenu = { track: asTrack(r, src), start: 'menu' })}>
        <span class="cover">
          {#if ok}<img src={coverUrl({ source: src, id: r.id! })} alt="" loading="lazy" />{:else}<span class="noimg"></span>{/if}
          {#if here}<span class="eq" class:still={paused()} aria-hidden="true"><i></i><i></i><i></i></span>{/if}
        </span>
        <span class="text">
          <strong>{r.name ?? r.file}</strong>
          <span class="sub">
            {#if r.missing}Missing file{:else if !r.id}Not matched yet{:else}<span class="artist">{r.artist ?? ''}</span>{#if r.album}<span class="album">{r.album}</span>{/if}{/if}
          </span>
        </span>
        {#if r.duration_secs}<span class="d">{clockText(r.duration_secs)}</span>{/if}
      </button>
      {#if ok}
        <RowHeart source={src} id={r.id!} name={r.name ?? r.file} onopen={() => (trackMenu = { track: asTrack(r, src), start: 'lists' })} />
      {/if}
    </div>
  </li>
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
  }

  /* Finamp's playlist heart: filled when it is a favourite. */
  .icon.fav :global(svg) {
    fill: currentColor;
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

  .barbtn.save {
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  .barbtn[aria-pressed='true'] {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
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

  /* Edit mode's Name and Spoken as, in place of the info beside the cover. */
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

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    /* On a phone the mini-player floats over the end of the list (as Finamp's now-playing bar): room to scroll past it. */
    padding-bottom: calc(32px + var(--mini-h, 0px) + env(safe-area-inset-bottom));
  }

  .head {
    display: grid;
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

  .actions button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .add-playing {
    justify-self: start;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-left: 6px;
    padding: 6px 12px;
    border: 1px dashed var(--border);
    border-radius: 999px;
    background: none;
    color: inherit;
    font: inherit;
    font-size: 0.9rem;
    cursor: pointer;
  }

  .related {
    margin: 0 8px 8px;
    padding: 10px 12px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--text) 5%, transparent);
    font-size: 14px;
  }

  .note {
    margin: 0;
    color: var(--muted);
    font-size: 13px;
  }

  .related h3 {
    margin: 8px 0 4px;
    font-size: 14px;
    font-weight: 500;
  }

  .related ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .related li {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0 8px;
    padding: 2px 0;
  }

  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font: inherit;
    cursor: pointer;
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
  .sect-head {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-height: 44px;
    padding: 0 12px 0 16px;
    border: 0;
    background: color-mix(in srgb, var(--text) 5%, transparent);
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .sect + .sect {
    margin-top: 6px;
  }

  .sname {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }

  .scount {
    flex: none;
    color: var(--muted);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }

  .sect-head :global(svg) {
    flex: none;
    transition: transform 150ms;
  }

  .sect-head.folded :global(svg) {
    transform: rotate(180deg);
  }

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
    /* Only rows on screen are drawn (a 500-track playlist opens fast). */
    content-visibility: auto;
    contain-intrinsic-size: auto 70px;
  }

  .rows li.off {
    opacity: 0.5;
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

  .slide {
    position: relative;
    display: flex;
    align-items: center;
    height: 100%;
    border-radius: 8px;
    background: var(--surface);
  }

  /* The playing track: tinted in the cover's colour (Finamp: primary at 30–35 %). */
  .rows li.now .slide {
    background: color-mix(in srgb, var(--art-accent, var(--accent)) 32%, var(--surface));
    transition: background 500ms;
  }

  .row {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 100%;
    padding: 0 4px 0 0;
    border: 0;
    border-radius: 8px;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .row:disabled {
    cursor: default;
  }

  .row:hover:not(:disabled) {
    background: color-mix(in srgb, var(--text) 7%, transparent);
  }

  .cover {
    position: relative;
    flex: none;
    width: 60px;
    height: 60px;
  }

  .cover img,
  .noimg {
    display: block;
    width: 100%;
    height: 100%;
    border-radius: 8px;
    object-fit: cover;
    background: var(--bg);
  }

  /* Finamp's mini visualiser on the playing track's cover, over a dark veil. */
  .eq {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 3px;
    border-radius: 8px;
    background: rgb(0 0 0 / 0.4);
  }

  .eq i {
    width: 4px;
    height: 15px;
    border-radius: 2px;
    background: white;
    transform-origin: bottom;
    animation: eq 900ms ease-in-out infinite alternate;
  }

  .eq i:nth-child(2) {
    animation-delay: -300ms;
  }

  .eq i:nth-child(3) {
    animation-delay: -600ms;
  }

  .eq.still i {
    animation: none;
    transform: scaleY(0.4);
  }

  @keyframes eq {
    from {
      transform: scaleY(0.25);
    }
    to {
      transform: scaleY(1);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .eq i {
      animation: none;
    }
  }

  .text {
    flex: 1;
    min-width: 0;
  }

  .text strong {
    display: -webkit-box;
    overflow: hidden;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    font-size: 15.5px;
    font-weight: 500;
    line-height: 1.1;
  }

  .sub {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
  }

  .artist {
    opacity: 0.75;
  }

  .album {
    margin-left: 10px;
    font-weight: 300;
    opacity: 0.6;
  }

  .d {
    flex: none;
    color: var(--muted);
    opacity: 0.85;
    font-size: 13px;
    font-variant-numeric: tabular-nums;
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
