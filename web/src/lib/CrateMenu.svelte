<script lang="ts">
  // A crate's menu (decision 0058; a genre is a crate of one condition): a long-press on its card on the Crates page, a
  // right-click there, or the ⋮ in its panel. The crate on the card (its picture or its albums' mosaic), Play, Shuffle
  // and Start radio as the large actions (no Play next or Add to queue: a crate's or a playlist's make no sense, the
  // owner) (its tracks as its favourites settings have them),
  // then the shared groups in the table's order (lib/menuTable): View; Save as playlist (a genre: Start a crate from
  // it); Download…, Sync to this device and Share; Edit (one's own); Delete (one's own, last). A word's (a genre, style,
  // mood or tag; decision 0097, administrators'): the sources that say it in its header, Rename everywhere and Merge
  // into… (its spelling, everywhere: the kind's spellings in Catalogue), and, opened from a record's words (`from`),
  // Remove from this record (the record's words of that kind set by hand without it).
  import { hiddenLibraries } from './listFilter.svelte';
  import Asking from './Asking.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import { crateRows, getCrate, getCrateAxes, setAlbumOverrides, type WordSource } from './api';
  import { me } from './me.svelte';
  import { WORD_FIELD_OF, mergeInto, sourceNames } from './words.svelte';
  import { inApp, type Downloadable } from './app.svelte';
  import DownloadStep from './DownloadStep.svelte';
  import { inOrder, type EntryId, type MenuKind } from './menuTable';
  import MenuTable from './MenuTable.svelte';
  import { CRATE, EDIT_CRATE, WORD_LABEL, crateArt, crateName, crateRadio, crates, deleteOwnCrate, genreOf, likeOf, playCrate, seeded, shuffleCrate, startCrate, wordOf, withoutFor } from './crates.svelte';
  import { isKept, setKept } from './kept.svelte';
  import { panel, viewPlaylist } from './panel.svelte';
  import { work } from './panelWork.svelte';
  import { prefs } from './prefs.svelte';
  import { linkTo } from './addresses';
  import { share } from './share';
  import { toast } from './toast.svelte';
  import { plural } from './insights.svelte';
  import { wordChip } from './quickCrate';
  import { grabCrate } from './quickCrates.svelte';

  let {
    id,
    onclose,
    onsave,
    from = null,
  }: {
    id: string;
    onclose: () => void;
    /** Its panel's own Make playlist, where it shows. */
    onsave?: () => void;
    /** A word's opened from a record's words: the record, and its words of that kind as it shows them. */
    from?: { album: string; words: string[] } | null;
  } = $props();

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
    crateRows(c, 0, 1, prefs.source || undefined, undefined, undefined, { without: withoutFor(c) })
      .then((r) => ((source = r.source), (saidBy = r.word?.sources ?? [])))
      .catch(() => {});
  });

  // A word of 0097's kinds (not a label): who says it, and its spelling changed by an administrator.
  const kindWord = $derived(word && word.kind !== 'label' ? { kind: word.kind, word: word.word } : null);
  let saidBy = $state<WordSource[]>([]);
  const wordAdmin = $derived(me.admin && kindWord !== null);
  let typed = $state('');
  let known = $state<string[]>([]);
  let knownWait = $state(false);
  function openRename() {
    typed = kindWord?.word ?? '';
    step = 'rename';
  }
  function openMerge() {
    typed = '';
    step = 'merge';
    const k = kindWord?.kind;
    if (!k || known.length) return;
    knownWait = true;
    getCrateAxes(prefs.source || undefined)
      .then(
        (axes) => (known = axes.find((x) => x.key === k)?.values ?? []),
        () => {},
      )
      .finally(() => (knownWait = false));
  }
  const mergeChips = $derived.by(() => {
    const q = typed.trim().toLowerCase();
    return known.filter((w) => w !== kindWord?.word && (!q || w.toLowerCase().includes(q))).slice(0, q ? 60 : 24);
  });
  /** Its spelling becomes `into` everywhere; the panel showing it shows `into`'s. */
  async function spellAs(into: string) {
    const w = kindWord;
    const to = into.trim().replace(/\s+/g, ' ');
    if (!w || !to || to === w.word || working) return;
    working = true;
    const shown = viewed;
    const ok = await mergeInto(w.kind, w.word, to);
    working = false;
    if (!ok) return;
    onclose();
    if (shown) viewPlaylist(`${CRATE}${w.kind}:${to}`);
  }
  function removeHere() {
    const [w, f] = [kindWord, from];
    if (!w || !f) return;
    void run(() => setAlbumOverrides([f.album], { [WORD_FIELD_OF[w.kind]]: f.words.filter((x) => x !== w.word) }), null, "It wasn't removed");
  }

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
  /** A word's crate (a genre, style, mood, tag or label) as the crate search's term, to refine (0.12: one way to make a
   *  crate; its Edit is the builder). */
  function grabWord() {
    const w = word;
    if (w === null) return;
    onclose();
    grabCrate([wordChip(w.kind, w.word)]);
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
  let step = $state<'menu' | 'download' | 'rename' | 'merge'>('menu');
  let all = $state<Downloadable[] | null>(null);
  /** The most a crate's Download… takes. */
  const MOST = 10000;
  async function download() {
    step = 'download';
    if (all) return;
    const got: Downloadable[] = [];
    try {
      for (let at = 0; at < MOST; at += 500) {
        const r = await crateRows(id, at, 500, prefs.source || undefined, undefined, undefined, { without: withoutFor(id) });
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
    {#if like !== null}<MenuEntry icon="crates" label="Start a crate from it" onclick={fromLike} />{/if}
  {:else if e === 'grabCrate'}
    {#if word !== null}<MenuEntry icon="crates" label="Grab a crate" onclick={grabWord} />{/if}
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
  {:else if e === 'renameWord'}
    {#if wordAdmin}<MenuEntry icon="pencil" label="Rename everywhere" onclick={openRename} />{/if}
  {:else if e === 'mergeWord'}
    {#if wordAdmin}<MenuEntry icon="arrowRight" label="Merge into…" onclick={openMerge} />{/if}
  {:else if e === 'removeWord'}
    {#if wordAdmin && from?.words.includes(kindWord?.word ?? '')}<MenuEntry icon="minus" label="Remove from this record" disabled={working} onclick={removeHere} />{/if}
  {:else if e === 'deleteCrate'}
    {#if own}<MenuEntry icon="trash" label="Delete crate" danger disabled={working} onclick={() => run(() => deleteOwnCrate(id), null, "The crate wasn't deleted")} />{/if}
  {/if}
{/snippet}

<MenuSheet
  label={name}
  {onclose}
  images={crateArt(id)}
  title={name}
  chips={[
    word !== null ? WORD_LABEL[word.kind] : like !== null ? 'Sounds like' : 'Crate',
    ...(tracks !== null ? [plural(tracks, 'track')] : []),
    ...(kindWord && saidBy.length ? [sourceNames(saidBy)] : []),
  ]}
  actions={step !== 'menu'
    ? []
    : inOrder(kind, [
        { icon: 'play', label: 'Play', disabled: working, onclick: () => run(() => playCrate(id, name, 'play'), null, 'Could not start playing') },
        { icon: 'shuffle', label: 'Shuffle', disabled: working, onclick: () => run(() => shuffleCrate(id, name), null, 'Could not start playing') },
        { icon: 'radio', label: 'Start radio', disabled: working, onclick: () => run(async () => crateRadio(id, name), null, "The radio didn't start") },
      ])}
  onback={step === 'menu' ? null : () => (step = 'menu')}
  {step}
>
  {#if step === 'download'}
    {#if all}<DownloadStep what={{ kind: 'tracks', name, tracks: all.map((t) => ({ source: t.source, id: t.id })) }} tracks={all} {onclose} />{:else}<Asking />{/if}
  {:else if step === 'rename'}
    <div class="step field">
      <!-- svelte-ignore a11y_autofocus -->
      <input type="text" aria-label="Spelled as" autofocus bind:value={typed} onkeydown={(e) => e.key === 'Enter' && void spellAs(typed)} />
    </div>
    <MenuEntry icon="check" label="Done" disabled={working || !typed.trim() || typed.trim() === kindWord?.word} onclick={() => void spellAs(typed)} />
  {:else if step === 'merge'}
    <div class="step field">
      <input type="search" placeholder="Find a word" aria-label="Find a word" bind:value={typed} />
    </div>
    {#if knownWait}
      <Asking />
    {:else}
      <div class="step chips">
        {#each mergeChips as w (w)}
          <button class="chip" disabled={working} onclick={() => void spellAs(w)}>{w}</button>
        {/each}
      </div>
    {/if}
  {:else}
    <!-- The shared groups, in the table's order (decision 0058, lib/menuTable). -->
    <MenuTable {kind} {entry} />
  {/if}
</MenuSheet>

<style>
  .step {
    padding: 0 16px 8px;
  }

  .field {
    display: grid;
  }

  /* Record Edit's field and chips (lib/RecordEdit). */
  input {
    min-height: 36px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    color: var(--text);
    font: inherit;
    font-size: 15px;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    min-height: 32px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg);
    color: inherit;
    font: inherit;
    font-size: 14px;
    cursor: pointer;
  }
</style>
