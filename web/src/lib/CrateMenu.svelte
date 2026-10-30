<script lang="ts">
  // A crate's menu (decision 0058; a genre is a crate of one condition): a long-press on its card on the Crates page, a
  // right-click there, or the ⋮ in its panel. The crate on the card (its picture or its albums' mosaic), Play, Shuffle
  // and Start radio as the large actions (no Play next or Add to queue: a crate's or a playlist's make no sense, the
  // owner) (its tracks as its favourites settings have them),
  // then the shared groups in the table's order (lib/menuTable): View; Save as playlist (a genre: Start a crate from
  // it); Download…, Sync to this device and Share; Edit (one's own); Delete (one's own, last).
  import { hiddenLibraries } from './listFilter.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import { crateRows, getCrate } from './api';
  import { inApp, type Downloadable } from './app.svelte';
  import DownloadStep from './DownloadStep.svelte';
  import { inOrder, type EntryId, type MenuKind } from './menuTable';
  import MenuTable from './MenuTable.svelte';
  import { CRATE, EDIT_CRATE, WORD_LABEL, crateArt, crateName, crateRadio, crates, deleteOwnCrate, genreOf, likeOf, playCrate, seeded, shuffleCrate, startCrate, wordOf } from './crates.svelte';
  import { isKept, setKept } from './kept.svelte';
  import { panel, viewPlaylist } from './panel.svelte';
  import { work } from './panelWork.svelte';
  import { prefs } from './prefs.svelte';
  import { linkTo, share } from './share';
  import { toast } from './toast.svelte';
  import { plural } from './insights.svelte';

  let { id, onclose, onsave }: { id: string; onclose: () => void; /** Its panel's own Make playlist, where it shows. */ onsave?: () => void } = $props();

  const name = $derived(crateName(id));
  const genre = $derived(genreOf(id));
  /** A style's, a mood's or a tag's (0.9.7): named by its kind, as a genre is. */
  const word = $derived(wordOf(id));
  const crate = $derived(crates.list?.find((c) => c.id === id) ?? null);
  const like = $derived(likeOf(id));
  /** One's own (a permanent crate, a genre and what sounds like a track or an album are the library's). */
  const own = $derived(!!crate && !crate.permanent && !seeded(id));
  const viewed = $derived(panel.viewed === CRATE + id);
  const tracks = $derived(crate?.stats?.tracks ?? null);

  // Where its tracks come from (Sync to this device follows that library's).
  let source = $state<string | null>(null);
  $effect(() => {
    const c = id;
    if (!c) return;
    crateRows(c, 0, 1, prefs.source || undefined, undefined, undefined, { without: hiddenLibraries() })
      .then((r) => (source = r.source))
      .catch(() => {});
  });

  let working = $state(false);
  async function run(what: () => Promise<unknown>, done: string | null, fail: string) {
    navigator.vibrate?.(8);
    working = true;
    try {
      await what();
      if (done) toast.show({ text: done, tone: 'done' });
      onclose();
    } catch (e) {
      toast.fail(e, fail);
    } finally {
      working = false;
    }
  }
  // Each reads what it needs before closing: closing lets go of `id` (its opener's state), so a view after it read null.
  function view() {
    const c = id;
    onclose();
    viewPlaylist(CRATE + c);
  }
  // Save as playlist: its panel, asking for the new playlist's name (its Make playlist).
  function saveAsPlaylist() {
    const c = id;
    const save = onsave;
    onclose();
    if (save) return save();
    work.making = { for: CRATE + c, state: { name: '', saving: false } };
    viewPlaylist(CRATE + c);
  }
  function fromGenre() {
    const g = genre;
    if (g === null) return;
    onclose();
    startCrate(g, [{ axis: 'genre', not: false, value: { kind: 'any_of', values: [g] } }]);
  }
  /** Sounds like's own crate: its seeds and how alike, as the server made them. */
  function fromLike() {
    const [c, n] = [id, name];
    onclose();
    getCrate(c)
      .then((d) => startCrate(n, d.conditions))
      .catch((e) => toast.fail(e, "The crate couldn't be started"));
  }
  function edit() {
    const c = id;
    onclose();
    viewPlaylist(EDIT_CRATE + c);
  }
  function shareIt() {
    const [c, n] = [id, name];
    onclose();
    share(n, n, linkTo('crates', CRATE + c));
  }

  // Download… (0.10): its tracks as it lists them, read when the step opens, a page at a time.
  let step = $state<'menu' | 'download'>('menu');
  let all = $state<Downloadable[] | null>(null);
  /** The most a crate's Download… takes. */
  const MOST = 10000;
  async function download() {
    step = 'download';
    if (all) return;
    const got: Downloadable[] = [];
    try {
      for (let at = 0; at < MOST; at += 500) {
        const r = await crateRows(id, at, 500, prefs.source || undefined, undefined, undefined, { without: hiddenLibraries() });
        for (const x of r.rows) {
          if (x.id && !x.missing) got.push({ source: r.source, id: x.id, name: x.name ?? x.file, artist: x.artist, album: x.album, album_id: x.album_id ?? null });
        }
        if (r.rows.length < 500 || at + 500 >= r.total) break;
      }
      all = got;
    } catch (e) {
      step = 'menu';
      toast.fail(e, "Its tracks couldn't be read");
    }
  }
  const kind = $derived<MenuKind>(genre !== null ? 'genre' : 'crate');
</script>

{#snippet entry(e: EntryId)}
  {#if e === 'view'}
    {#if !viewed}<MenuEntry icon="squareArrowOutUpRight" label="View" onclick={view} />{/if}
  {:else if e === 'startCrate'}
    {#if genre !== null}
      <MenuEntry icon="crates" label="Start a crate from it" onclick={fromGenre} />
    {:else if like !== null}
      <MenuEntry icon="crates" label="Start a crate from it" onclick={fromLike} />
    {/if}
  {:else if e === 'saveAsPlaylist'}
    {#if genre === null && like === null}<MenuEntry icon="intoBox" label="Save as playlist" onclick={saveAsPlaylist} />{/if}
  {:else if e === 'download'}
    <MenuEntry icon="arrowDownToLine" label="Download…" onclick={download} />
  {:else if e === 'keep'}
    {#if inApp && source && genre === null}
      {@const s = source}
      {@const on = isKept('crate', s, id)}
      <MenuEntry icon="download" label="Sync to this device" switched={on} onclick={() => setKept('crate', s, id, !on, name)} />
    {/if}
  {:else if e === 'share'}
    <MenuEntry icon="share" label="Share" onclick={shareIt} />
  {:else if e === 'edit'}
    {#if own}<MenuEntry icon="pencil" label="Edit" onclick={edit} />{/if}
  {:else if e === 'deleteCrate'}
    {#if own}<MenuEntry icon="trash" label="Delete crate" danger disabled={working} onclick={() => run(() => deleteOwnCrate(id), null, "The crate wasn't deleted")} />{/if}
  {/if}
{/snippet}

<MenuSheet
  label={name}
  {onclose}
  images={crateArt(id)}
  title={name}
  chips={[word !== null ? WORD_LABEL[word.kind] : like !== null ? 'Sounds like' : 'Crate', ...(tracks !== null ? [plural(tracks, 'track')] : [])]}
  actions={step !== 'menu'
    ? []
    : inOrder(kind, [
        { icon: 'play', label: 'Play', disabled: working, onclick: () => run(() => playCrate(id, name, 'play'), null, 'Could not start playing') },
        { icon: 'shuffle', label: 'Shuffle', disabled: working, onclick: () => run(() => shuffleCrate(id, name), null, 'Could not start playing') },
        { icon: 'radio', label: 'Start radio', disabled: working, onclick: () => run(() => crateRadio(id, name), null, "The radio didn't start") },
      ])}
  onback={step === 'menu' ? null : () => (step = 'menu')}
  {step}
>
  {#if step === 'download'}
    {#if all}<DownloadStep what={{ kind: 'tracks', name, tracks: all.map((t) => ({ source: t.source, id: t.id })) }} tracks={all} {onclose} />{/if}
  {:else}
    <!-- The shared groups, in the table's order (decision 0058, lib/menuTable). -->
    <MenuTable {kind} {entry} />
  {/if}
</MenuSheet>
