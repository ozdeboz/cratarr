<script lang="ts">
  // An opened playlist (decision 0023), in the panel beside the page or in its own layer, styled as Finamp's playlist
  // screen (lib/components/AlbumScreen) in less room: a bar with ✕ and the menu; the covers beside the name, "N tracks
  // (length)" and the genres; Play, Shuffle, Play next and Add to queue as icons over labels; what Insights knows of it
  // (Related, a section until it gets a tab); then the tracks in file order. A tap on a track plays the playlist from
  // there on the chosen output, a swipe left plays it next, a long-press is its menu; the heart adds it to playlists
  // (long-press: favourite). Lines the server does not know, or files that are missing, show by file name, dimmed.
  // Edit mode (the pencil; Finamp's playlist edit screen): rows get the reorder handle and ✕ (a swipe left removes too),
  // removed tracks wait in their own section to be restored, and Save writes the playlist once (Cancel drops it).
  import { coverUrl, editPlaylist, incomingCoverUrl, playlistCovers, playlistRows, queueTrack, setSpoken, spokenNames, type PlaylistRow } from './api';
  import { Reorder } from './reorder.svelte';
  import { isPlaylistFavorite, loadFavorites, loadPlaylistFavorites, togglePlaylistFavorite } from './favourites.svelte';
  import Icon, { type IconName } from './Icon.svelte';
  import { insights, plural } from './insights.svelte';
  import { refresh } from './live.svelte';
  import { enqueue, type LocalTrack } from './localPlayer.svelte';
  import { longpress } from './longpress';
  import { isLocal, playPlaylistFrom, setPlayingMember, shownPlaylists, startPlaylist } from './output.svelte';
  import { closePlaylist, openPlaylist, panel } from './panel.svelte';
  import PlaylistMenu from './PlaylistMenu.svelte';
  import { prefs } from './prefs.svelte';
  import { clockText, paused, playing } from './remote.svelte';
  import RowHeart from './RowHeart.svelte';
  import { swipe } from './swipe';
  import { toast } from './toast.svelte';
  import TrackMenu, { type MenuTrack } from './TrackMenu.svelte';
  import type { PlayMode } from './api';

  const id = $derived(panel.open ?? '');
  const p = $derived(shownPlaylists().find((x) => x.id === id) ?? null);
  const np = $derived(playing());

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
    if (!want) return;
    if (want !== shownFor) {
      rows = null;
      error = null;
    }
    playlistRows(want, prefs.source || undefined).then(
      (r) => {
        if (want !== id) return;
        shownFor = want;
        rows = r.rows;
        source = r.source;
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
    if (!want) return;
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
  const tracksOf = (list: PlaylistRow[]): LocalTrack[] =>
    list.map((x) => ({ source, id: x.id!, name: x.name ?? x.file, artist: x.artist, album: x.album, album_id: null }));

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
  const asTrack = (r: PlaylistRow): MenuTrack => ({ source, id: r.id!, name: r.name ?? r.file, artist: r.artist, album: r.album, album_id: null });

  // Finamp's playback actions: a new queue (Play, Shuffle), or the whole playlist next or at the end of the queue.
  const ACTIONS: { mode: PlayMode; icon: IconName; label: string }[] = [
    { mode: 'play', icon: 'play', label: 'Play' },
    { mode: 'shuffle', icon: 'shuffle', label: 'Shuffle' },
    { mode: 'next', icon: 'playNext', label: 'Play next' },
    { mode: 'last', icon: 'playlists', label: 'Add to queue' },
  ];
  async function act(mode: PlayMode) {
    navigator.vibrate?.(8);
    try {
      await startPlaylist(id, mode);
      if (mode === 'next' || mode === 'last') toast.show({ text: mode === 'next' ? 'Plays next.' : 'Added to the queue.', tone: 'done' });
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    }
  }

  async function playFrom(r: PlaylistRow) {
    if (!r.id || r.missing) return;
    navigator.vibrate?.(8);
    try {
      await playPlaylistFrom(id, source, tracksOf(playable), playable.indexOf(r));
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    }
  }

  // A swipe left plays the track next (Finamp's default swipe on a playlist's rows); the row springs back.
  function playNext(r: PlaylistRow) {
    (async () => {
      try {
        if (isLocal()) enqueue(tracksOf([r]), 'next');
        else await queueTrack(r.id!, 'next', source);
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
  // the names the playlist is also spoken as (decision 0015), at the top as Finamp's edit screen has its name.
  type Item = { r: PlaylistRow; at: number };
  let editing = $state<{ rows: Item[]; removed: Item[]; spoken: string; saving: boolean } | null>(null);
  const editingFor = $derived(editing ? id : null);
  $effect(() => {
    // Another playlist opened (or this one closed): the edit is dropped.
    if (editingFor !== null && editingFor !== id) editing = null;
  });
  const tracksChanged = $derived(!!editing && (editing.removed.length > 0 || editing.rows.some((x, i) => x.at !== i)));
  const spokenChanged = $derived(!!editing && spokenNames(editing.spoken).join('\n') !== (p?.spoken ?? []).join('\n'));
  function startEdit() {
    if (!rows) return;
    navigator.vibrate?.(8);
    editing = { rows: rows.map((r, at) => ({ r, at })), removed: [], spoken: (p?.spoken ?? []).join(', '), saving: false };
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
    if (!tracksChanged && !spokenChanged) return void (editing = null);
    editing.saving = true;
    try {
      if (tracksChanged)
        await editPlaylist(
          id,
          rows.map((r) => r.entry),
          editing.rows.map((x) => x.r.entry),
        );
      if (spokenChanged) await setSpoken(id, spokenNames(editing.spoken));
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
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && !menuOpen && !trackMenu && !editing && closePlaylist();
    addEventListener('keydown', esc);
    return () => removeEventListener('keydown', esc);
  });
</script>

<section class="view" aria-label={id}>
  <div class="bar">
    {#if editing}
      <button class="barbtn" onclick={() => (editing = null)} disabled={editing.saving}>Cancel</button>
      <button class="barbtn save" onclick={save} disabled={editing.saving} aria-busy={editing.saving}>Save</button>
    {:else}
      <button class="icon" aria-label="Close the playlist" onclick={closePlaylist}><Icon name="close" size={24} /></button>
      <span class="end">
        <button class="icon" class:fav={isPlaylistFavorite(id)} aria-pressed={isPlaylistFavorite(id)} aria-label="Favourite" onclick={() => togglePlaylistFavorite(id)}
          ><Icon name="heart" size={24} /></button
        >
        {#if rows && p}<button class="icon" aria-label={`Edit ${id}`} onclick={startEdit}><Icon name="pencil" size={22} /></button>{/if}
        <button class="icon" aria-label={`Menu of ${id}`} onclick={() => (menuOpen = true)}><Icon name="moreVert" size={24} /></button>
      </span>
    {/if}
  </div>

  <div class="scroll" bind:this={scroller}>
    <header class="head">
      <div class="top">
        <div class="art" class:mosaic={covers.length === 4}>
          {#each covers as c, i (i)}<img src={c} alt="" />{:else}<span class="noart"><Icon name="albums" size={48} /></span>{/each}
        </div>
        <div class="info">
          <h2>{id}</h2>
          <p class="line"><Icon name="music" size={20} /><span>{p ? plural(p.count, 'track') : rows ? plural(rows.length, 'track') : 'Loading…'}{#if length}&nbsp;({clockText(length)}){/if}</span></p>
          {#if point}
            <p class="line"><Icon name="swatch" size={20} />{#if point.genres.length}<span class="genres">{#each point.genres as g (g)}<span>{g}</span>{/each}</span>{:else}<span>No genres</span>{/if}</p>
          {/if}
          {#if p?.dead}<p class="line dead"><span>{p.dead} missing</span></p>{/if}
        </div>
      </div>
      {#if editing}
        <label class="field">
          <span>Spoken as</span>
          <input type="text" placeholder="Other names, comma-separated" bind:value={editing.spoken} disabled={editing.saving} />
        </label>
      {/if}
      {#if !editing}
      <div class="actions" role="group" aria-label="Play">
        {#each ACTIONS as a (a.mode)}
          <button onclick={() => act(a.mode)} disabled={!playable.length}><Icon name={a.icon} size={28} /><span>{a.label}</span></button>
        {/each}
      </div>
      {#if np && np.addable && p && !p.member}
        <button class="add-playing" onclick={addPlaying}><Icon name="plus" size={18} /><span>Add what's playing</span></button>
      {/if}
      {/if}
    </header>

    {#if editing}
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
              <li><button class="link" onclick={() => openPlaylist(c.name, { replace: true })}>{c.name}</button><span class="note">{c.shares}</span></li>
            {/each}
          </ul>
        {/if}
      </section>
    {/if}

    {#if editing}
      <!-- the edit list is above -->
    {:else if error}
      <p class="error" role="alert">{error}</p>
    {:else if rows && !rows.length}
      <p class="empty">No tracks yet.</p>
    {:else if rows}
      <ol class="rows">
        {#each rows as r, i (i)}
          {@const ok = !!r.id && !r.missing}
          {@const here = ok && !!np && np.id === r.id}
          <li class:off={!ok} class:now={here}>
            <span class="behind" aria-hidden="true"><Icon name="playNext" size={40} /><span>Play next</span></span>
            <div class="slide" use:swipe={{ enabled: ok, onswipe: () => playNext(r) }}>
              <button class="row" disabled={!ok} onclick={() => playFrom(r)} use:longpress={() => ok && (trackMenu = { track: asTrack(r), start: 'menu' })}>
                <span class="cover">
                  {#if ok}<img src={coverUrl({ source, id: r.id! })} alt="" loading="lazy" />{:else}<span class="noimg"></span>{/if}
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
                <RowHeart {source} id={r.id!} name={r.name ?? r.file} onopen={() => (trackMenu = { track: asTrack(r), start: 'lists' })} />
              {/if}
            </div>
          </li>
        {/each}
      </ol>
    {:else}
      <p class="empty">Loading…</p>
    {/if}
  </div>
</section>

{#if menuOpen && p}
  <PlaylistMenu playlist={p} onclose={() => (menuOpen = false)} />
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
    background: var(--bg);
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

  /* Edit mode's fields, at the top as Finamp's edit screen has its name field. */
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
    background: var(--surface);
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
    background: var(--surface);
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
    background: var(--bg);
  }

  /* The playing track: tinted in the cover's colour (Finamp: primary at 30–35 %). */
  .rows li.now .slide {
    background: color-mix(in srgb, var(--art-accent, var(--accent)) 32%, var(--bg));
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
    background: var(--surface);
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
