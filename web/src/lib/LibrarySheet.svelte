<script lang="ts" module>
  import type { AlbumsSource, IncomingSource, LibraryInfo, ListedAlbum } from './api';
  import { jobs } from './jobs.svelte';

  /** What an album's sheet offers (decision 0034): moves into the server's other used libraries, Replace for an
   *  upgrade, Delete; each only when Cratarr can write the album's folder, and with a job in the conversion queue only
   *  Cancel. */
  export type AlbumOffer = {
    /** The album's library, when known. */
    from: LibraryInfo | null;
    /** The libraries it can move into. */
    targets: LibraryInfo[];
    /** Into Music the old way (Keep), where the libraries are not known (the panel's Incoming album). */
    keepable: boolean;
    replaceable: boolean;
    deletable: boolean;
    job: number | null;
  };

  /** For an album of the Library's list. One already in Music (an upgrade or a duplicate) does not move there again:
   *  an upgrade replaces. */
  export function offerIn(s: AlbumsSource, a: ListedAlbum): AlbumOffer {
    const deletable = s.deletable.includes(a.id);
    const verdict = s.verdicts[a.id];
    return {
      from: s.library_info.find((l) => l.id === a.library) ?? null,
      targets: deletable ? s.library_info.filter((l) => l.id !== a.library && !(verdict && l.use === 'music')) : [],
      keepable: false,
      replaceable: deletable && verdict?.kind === 'upgrade',
      deletable,
      job: jobs.of(s.id, a.id)?.id ?? null,
    };
  }

  /** For an Incoming album (the panel's): Keep for one not in Music yet (no verdict), Replace for an upgrade. */
  export function offerOf(s: IncomingSource, id: string): AlbumOffer {
    const deletable = s.deletable.includes(id);
    return {
      from: null,
      targets: [],
      keepable: deletable && !s.verdicts?.[id],
      replaceable: deletable && s.verdicts?.[id]?.kind === 'upgrade',
      deletable,
      job: jobs.of(s.id, id)?.id ?? null,
    };
  }
</script>

<script lang="ts">
  // An album's sheet (decision 0021) for what its library allows (0034): the Library's menu (Play, View, Move to
  // another library, Replace in Music, Delete), or straight at one of its questions from the album panel's top bar.
  // Each question is asked in place. Deleting from the primary library (Music) asks once more.
  import {
    albumCoverUrl,
    cancelJob,
    deleteAlbum,
    keepIncoming,
    keepPreview,
    moveAlbum,
    movePreview,
    replaceIncoming,
    replacePreview,
    type AlbumMoved,
    type KeepPreview,
    type ReplacePreview,
  } from './api';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { albumGone, albumSubject, viewPlaylist } from './panel.svelte';
  import { toast } from './toast.svelte';
  import { withUndo } from './undo.svelte';
  import { playListedAlbum } from './albums.svelte';

  type Ask = 'delete' | 'keep' | 'replace';
  let {
    source,
    album: a,
    offer,
    trashDays,
    music,
    start = 'menu',
    onclose,
    ongone,
    onundone,
  }: {
    source: string;
    album: Pick<ListedAlbum, 'id' | 'title' | 'artist'> & Partial<ListedAlbum>;
    offer: AlbumOffer;
    trashDays: number;
    /** The primary library (Music), by its own name: Keep and Replace put albums in it. */
    music: string;
    /** The menu, or one question straight away (then there is no menu to go back to). */
    start?: 'menu' | Ask;
    onclose: () => void;
    /** The album left its library (moved, replaced or deleted; not when queued to convert first). */
    ongone?: () => void;
    /** Its Undo went through. */
    onundone?: () => void;
  } = $props();

  // Said in the question when the album leaves Incoming and no Lidarr is set to be told to stop fetching it.
  const NO_LIDARR = "Lidarr isn't set up in Settings, so it may fetch this album again.";
  /** Where the album is, for the Undo's words, and whether that is the primary library (Music). */
  // svelte-ignore state_referenced_locally
  const fromName = offer.from?.name ?? 'Incoming';
  // svelte-ignore state_referenced_locally
  const primary = offer.from?.use === 'music';

  // svelte-ignore state_referenced_locally
  let ask = $state<false | Ask | 'move' | 'really'>(start === 'menu' ? false : start);
  /** The move's library; none for Keep (into Music, the old way). */
  let to = $state<LibraryInfo | null>(null);
  let keep = $state<KeepPreview | null>(null);
  let swap = $state<ReplacePreview | null>(null);
  let year = $state('');
  let working = $state(false);
  const into = $derived(to?.name ?? music);

  async function askReplace() {
    ask = 'replace';
    swap = null;
    year = '';
    try {
      swap = await replacePreview(source, a.id);
    } catch (e) {
      toast.fail(e, 'Could not pair the tracks');
      onclose();
    }
  }

  const preview = (y?: string) => (to ? movePreview(source, a.id, to.id, y) : keepPreview(source, a.id, y));
  async function askMove(target: LibraryInfo | null) {
    ask = target ? 'move' : 'keep';
    to = target;
    keep = null;
    year = '';
    try {
      keep = await preview();
    } catch (e) {
      toast.fail(e, 'Could not work out the names');
      onclose();
    }
  }

  // Opened at a question: its preview is asked for at once.
  // svelte-ignore state_referenced_locally
  if (start === 'keep') askMove(null);
  // svelte-ignore state_referenced_locally
  else if (start === 'replace') askReplace();

  // With a year typed in, show what the names would be.
  async function tryYear() {
    if (!/^\d{4}$/.test(year.trim())) return;
    try {
      if (ask === 'replace') swap = await replacePreview(source, a.id, year.trim());
      else keep = await preview(year.trim());
    } catch {
      /* keep the question open */
    }
  }

  async function act(run: () => Promise<void>, failed: string) {
    if (working) return;
    working = true;
    try {
      await run();
    } catch (e) {
      toast.fail(e, failed);
    } finally {
      working = false;
      onclose();
    }
  }

  // Done: the album leaves its library. Queued (hi-res files to convert first): its row shows the progress instead,
  // and the queue says when it is done.
  function moved(r: AlbumMoved, words: string) {
    if (r.queued) {
      jobs.poke();
      return;
    }
    gone();
    withUndo(words, [r.undo], { undone: `${r.title} is back in ${fromName}.`, after: onundone });
  }
  function gone() {
    ongone?.();
    albumGone(source, a.id);
  }

  const moveIt = () =>
    act(async () => {
      const y = year.trim() || undefined;
      const r = to ? await moveAlbum(source, a.id, to.id, y) : await keepIncoming(source, a.id, y);
      moved(r, `Moved ${r.title} to ${into}.`);
    }, 'Could not move the album');
  const replaceIt = () =>
    act(async () => {
      const r = await replaceIncoming(source, a.id, year.trim() || undefined);
      moved(r, `Replaced ${r.title} in ${music}.`);
    }, 'Could not replace the album');
  const remove = () =>
    act(async () => {
      const { undo, title } = await deleteAlbum(source, a.id);
      gone();
      withUndo(`Deleted ${title}.`, [undo], { undone: `${title} is back in ${fromName}.`, after: onundone });
    }, 'Could not delete the album');

  async function cancel(id: number) {
    onclose();
    try {
      await cancelJob(id);
      await jobs.poke();
    } catch (e) {
      toast.fail(e, 'Could not cancel it');
    }
  }
</script>

<!-- The album's sheet, as every menu (decision 0021). -->
<MenuSheet
  label={a.title}
  {onclose}
  images={[albumCoverUrl(source, a.id)]}
  title={a.title}
  chips={[a.artist ?? 'Unknown artist']}
  actions={ask || !a.tracks
    ? []
    : [
        {
          icon: 'play',
          label: 'Play',
          onclick: () => {
            onclose();
            playListedAlbum(source, a as ListedAlbum);
          },
        },
      ]}
  onback={ask && start === 'menu' ? () => (ask = false) : null}
  step={ask || 'menu'}
>
  {#if !ask}
    <div role="menu">
      <MenuEntry
        icon="squareArrowOutUpRight"
        label="View"
        onclick={() => {
          onclose();
          viewPlaylist(albumSubject(source, a.id));
        }}
      />
      {#if offer.job !== null}
        {@const id = offer.job}
        <MenuEntry icon="close" label="Cancel conversion" danger onclick={() => cancel(id)} />
      {:else}
        {#if offer.keepable}
          <MenuEntry icon="intoBox" label={`Move to ${music}`} onclick={() => askMove(null)} />
        {/if}
        {#each offer.targets as t (t.id)}
          <MenuEntry icon="intoBox" label={`Move to ${t.name}`} onclick={() => askMove(t)} />
        {/each}
      {/if}
      {#if offer.replaceable}
        <MenuEntry icon="swap" label={`Replace in ${music}`} onclick={askReplace} />
      {/if}
      {#if offer.deletable && offer.job === null}
        <MenuEntry icon="trash" label="Delete album" danger onclick={() => (ask = 'delete')} />
      {/if}
    </div>
  {:else if ask === 'keep' || ask === 'move'}
    <div class="form">
      {#if !keep}
        <p class="ask">Working out the names…</p>
      {:else if keep.refused?.kind === 'needs_year'}
        <p class="ask">The tags don't say when {a.title} came out. Which year?</p>
        <input class="year" inputmode="numeric" maxlength="4" placeholder="Year" aria-label="Release year" bind:value={year} oninput={tryYear} />
      {:else if keep.refused}
        <p class="ask">Can't move {a.title}: {keep.refused.detail}</p>
      {:else}
        <p class="ask">
          Move {a.title} into {into} as <strong>{keep.folder}</strong> ({keep.files} {keep.files === 1 ? 'file' : 'files'})?{#if keep.convert}
            {` ${keep.convert} hi-res ${keep.convert === 1 ? 'file is' : 'files are'} converted to 16-bit/44.1 kHz first.`}{/if}{#if !keep.acquisition && !primary}
            {` ${NO_LIDARR}`}{/if}
        </p>
      {/if}
      <div class="actions">
        {#if keep && (!keep.refused || keep.refused.kind === 'needs_year')}
          <button class="primary" disabled={working || !!keep.refused} aria-busy={working} onclick={moveIt}>Move to {into}</button>
        {/if}
        <button onclick={onclose}>Cancel</button>
      </div>
    </div>
  {:else if ask === 'replace'}
    <div class="form">
      {#if !swap}
        <p class="ask">Pairing the tracks…</p>
      {:else if swap.refused?.kind === 'needs_year'}
        <p class="ask">The tags don't say when {a.title} came out. Which year?</p>
        <input class="year" inputmode="numeric" maxlength="4" placeholder="Year" aria-label="Release year" bind:value={year} oninput={tryYear} />
      {:else if swap.refused?.kind === 'unmatched'}
        <p class="ask">Can't replace {swap.old}: no match for {swap.refused.detail.join(', ')}.</p>
      {:else if swap.refused}
        <p class="ask">Can't replace {swap.old}: {swap.refused.detail}</p>
      {:else}
        <p class="ask">
          Replace <strong>{swap.old}</strong> with this copy ({swap.tracks} {swap.tracks === 1 ? 'track' : 'tracks'}{swap.added
            ? `, ${swap.added} new`
            : ''}){#if swap.folder !== swap.old}, as <strong>{swap.folder}</strong>{/if}? The old copy stays in the trash for {trashDays}
          {trashDays === 1 ? 'day' : 'days'}.{#if swap.convert}
            {` ${swap.convert} hi-res ${swap.convert === 1 ? 'file is' : 'files are'} converted to 16-bit/44.1 kHz first.`}{/if}{#if !swap.acquisition}
            {` ${NO_LIDARR}`}{/if}
        </p>
      {/if}
      <div class="actions">
        {#if swap && (!swap.refused || swap.refused.kind === 'needs_year')}
          <button class="primary" disabled={working || !!swap.refused} aria-busy={working} onclick={replaceIt}>Replace</button>
        {/if}
        <button onclick={onclose}>Cancel</button>
      </div>
    </div>
  {:else if ask === 'delete'}
    <div class="form">
      <p class="ask">Delete {a.title}? It stays in the trash for {trashDays} {trashDays === 1 ? 'day' : 'days'}.</p>
      <div class="actions">
        <button class="danger solid" disabled={working} aria-busy={working} onclick={() => (primary ? (ask = 'really') : remove())}>Delete album</button>
        <button onclick={onclose}>Cancel</button>
      </div>
    </div>
  {:else}
    <!-- The primary library's extra step (decision 0034). -->
    <div class="form">
      <p class="ask">{a.title} is in {music}, your main library. Delete it from there?</p>
      <div class="actions">
        <button class="danger solid" disabled={working} aria-busy={working} onclick={remove}>Delete from {music}</button>
        <button onclick={onclose}>Cancel</button>
      </div>
    </div>
  {/if}
</MenuSheet>

<style>
  .form {
    display: grid;
    gap: 12px;
    padding: 0 16px 8px;
  }

  .ask {
    margin: 0;
    font-size: 0.92rem;
  }

  .actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .actions button {
    min-height: 40px;
    padding: 0 14px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }

  .actions button.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  .year {
    min-height: 40px;
    width: 7em;
    padding: 0 12px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
  }

  .actions button.danger.solid {
    background: var(--danger);
    border-color: var(--danger);
    color: #fff;
    font-weight: 600;
  }

  .actions button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
