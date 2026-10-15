<script lang="ts" module>
  import type { IncomingSource } from './api';
  import { jobs } from './jobs.svelte';

  /** What Incoming offers for one album: Keep for one not in Music yet (no verdict), Replace for an upgrade, both and
   *  Delete only when Cratarr can write the album's folder; with a job in the conversion queue only Cancel. */
  export type IncomingOffer = { deletable: boolean; keepable: boolean; replaceable: boolean; job: number | null };

  /** What Incoming offers for album `id` of a server's list. */
  export function offerOf(s: IncomingSource, id: string): IncomingOffer {
    const deletable = s.deletable.includes(id);
    return {
      deletable,
      keepable: deletable && !s.verdicts?.[id],
      replaceable: deletable && s.verdicts?.[id]?.kind === 'upgrade',
      job: jobs.of(s.id, id)?.id ?? null,
    };
  }
</script>

<script lang="ts">
  // An Incoming album's sheet (decision 0021): the Incoming page's menu (Play, View, Keep or Replace in Music, Delete),
  // or straight at one of its questions from the album panel's top bar. Each question is asked in place.
  import {
    albumCoverUrl,
    cancelJob,
    deleteIncoming,
    keepIncoming,
    keepPreview,
    replaceIncoming,
    replacePreview,
    type AlbumMoved,
    type KeepPreview,
    type ListedAlbum,
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
    offer: IncomingOffer;
    trashDays: number;
    /** The library Keep and Replace put albums in, by its own name. */
    music: string;
    /** The menu, or one question straight away (then there is no menu to go back to). */
    start?: 'menu' | Ask;
    onclose: () => void;
    /** The album left Incoming (kept, replaced or deleted; not when queued to convert first). */
    ongone?: () => void;
    /** Its Undo went through. */
    onundone?: () => void;
  } = $props();

  // Said in the Keep and Replace questions when no Lidarr is set to be told to stop fetching the album.
  const NO_LIDARR = "Lidarr isn't set up in Settings, so it may fetch this album again.";

  // svelte-ignore state_referenced_locally
  let ask = $state<false | Ask>(start === 'menu' ? false : start);
  let keep = $state<KeepPreview | null>(null);
  let swap = $state<ReplacePreview | null>(null);
  let year = $state('');
  let working = $state(false);

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

  async function askKeep() {
    ask = 'keep';
    keep = null;
    year = '';
    try {
      keep = await keepPreview(source, a.id);
    } catch (e) {
      toast.fail(e, 'Could not work out the names');
      onclose();
    }
  }

  // Opened at a question: its preview is asked for at once.
  // svelte-ignore state_referenced_locally
  if (start === 'keep') askKeep();
  // svelte-ignore state_referenced_locally
  else if (start === 'replace') askReplace();

  // With a year typed in, show what the names would be.
  async function tryYear() {
    if (!/^\d{4}$/.test(year.trim())) return;
    try {
      if (ask === 'replace') swap = await replacePreview(source, a.id, year.trim());
      else keep = await keepPreview(source, a.id, year.trim());
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

  // Done: the album leaves Incoming. Queued (hi-res files to convert first): its row shows the progress instead, and
  // the queue says when it is done.
  function moved(r: AlbumMoved, verb: string) {
    if (r.queued) {
      jobs.poke();
      return;
    }
    gone();
    withUndo(`${verb} ${r.title} in ${music}.`, [r.undo], { undone: `${r.title} is back in Incoming.`, after: onundone });
  }
  function gone() {
    ongone?.();
    albumGone(source, a.id);
  }

  const keepIt = () => act(async () => moved(await keepIncoming(source, a.id, year.trim() || undefined), 'Kept'), 'Could not keep the album');
  const replaceIt = () => act(async () => moved(await replaceIncoming(source, a.id, year.trim() || undefined), 'Replaced'), 'Could not replace the album');
  const remove = () =>
    act(async () => {
      const { undo, title } = await deleteIncoming(source, a.id);
      gone();
      withUndo(`Deleted ${title}.`, [undo], { undone: `${title} is back in Incoming.`, after: onundone });
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
      {:else if offer.keepable}
        <MenuEntry icon="check" label={`Keep in ${music}`} onclick={askKeep} />
      {/if}
      {#if offer.replaceable}
        <MenuEntry icon="swap" label={`Replace in ${music}`} onclick={askReplace} />
      {/if}
      {#if offer.deletable && offer.job === null}
        <MenuEntry icon="trash" label="Delete album" danger onclick={() => (ask = 'delete')} />
      {/if}
    </div>
  {:else if ask === 'keep'}
    <div class="form">
      {#if !keep}
        <p class="ask">Working out the names…</p>
      {:else if keep.refused?.kind === 'needs_year'}
        <p class="ask">The tags don't say when {a.title} came out. Which year?</p>
        <input class="year" inputmode="numeric" maxlength="4" placeholder="Year" aria-label="Release year" bind:value={year} oninput={tryYear} />
      {:else if keep.refused}
        <p class="ask">Can't keep {a.title}: {keep.refused.detail}</p>
      {:else}
        <p class="ask">
          Move {a.title} into {music} as <strong>{keep.folder}</strong> ({keep.files} {keep.files === 1 ? 'file' : 'files'})?{#if keep.convert}
            {` ${keep.convert} hi-res ${keep.convert === 1 ? 'file is' : 'files are'} converted to 16-bit/44.1 kHz first.`}{/if}{#if !keep.acquisition}
            {` ${NO_LIDARR}`}{/if}
        </p>
      {/if}
      <div class="actions">
        {#if keep && (!keep.refused || keep.refused.kind === 'needs_year')}
          <button class="primary" disabled={working || !!keep.refused} aria-busy={working} onclick={keepIt}>Keep in {music}</button>
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
  {:else}
    <div class="form">
      <p class="ask">Delete {a.title}? It stays in the trash for {trashDays} {trashDays === 1 ? 'day' : 'days'}.</p>
      <div class="actions">
        <button class="danger solid" disabled={working} aria-busy={working} onclick={remove}>Delete album</button>
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
