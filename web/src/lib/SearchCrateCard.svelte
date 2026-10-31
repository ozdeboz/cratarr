<script lang="ts">
  // A search's words read as a crate (0.12; the owner, 2026-10-30: "a crate card first"): where they read as two terms
  // or more of two kinds or more (lib/quickCrate crateOfLine; read once, lib/globalSearch `crate`), the first result is
  // the crate they make. One card for global search's results and every page's own (0.13; the owner, 2026-10-31), shown
  // only once its count is in and it holds a track (crateShows): nothing for one word, one kind or an empty crate,
  // nothing while it loads. Its look is Home's week card (the owner, 2026-10-31: "A · Week", lib/CoverCard): a strip of
  // its covers, its sentence as the heading, its terms as the week's pills, each opening its menu (Leave out there too),
  // "+ word" for another; Play, Grab this crate (the panel's builder, lib/quickCrates grabCrate: the one place a crate is
  // shown and built) and its count.
  import { untrack } from 'svelte';
  import { coverUrl, previewCrate, search, type PlaylistRow } from './api';
  import CoverCard from './CoverCard.svelte';
  import { recordFirsts } from './crateRecords';
  import { globalSearch } from './globalSearch.svelte';
  import Icon from './Icon.svelte';
  import { plural } from './insights.svelte';
  import { hiddenLibraries } from './listFilter.svelte';
  import { playCollection } from './play.svelte';
  import { prefs } from './prefs.svelte';
  import QuickTermSheet from './QuickTermSheet.svelte';
  import { TERM_KINDS, conditionsOf, crateShows, nameOf, readLine, readOnto, seedsOf, sentenceOf, type Chip, type Found, type Modes } from './quickCrate';
  import { grabCrate } from './quickCrates.svelte';
  import { words } from './search';
  import { toast } from './toast.svelte';

  let { q }: { q: string } = $props();

  /** How many of each kind a phrase's readings offer, as the crate search asks. */
  const EACH = 3;
  const ask = (phrase: string): Promise<Found> =>
    search(phrase, [...TERM_KINDS], EACH).then((r) => TERM_KINDS.flatMap((k) => r.groups[k]?.items.slice(0, EACH) ?? []));
  /** Its first tracks asked: enough for six covers of six records. A crate's own order comes a record at a time, its
   *  tracks together: 24 were two hip hop albums, fewer than the strip's four, and the card showed no covers (the
   *  owner, 2026-10-31). */
  const FIRST = 200;
  const previewOf = (c: readonly Chip[], m: Modes) =>
    previewCrate(conditionsOf(c, undefined, m), FIRST, prefs.source || undefined, { without: hiddenLibraries() });

  type Shown = { chips: Chip[]; modes: Modes; preview: { source: string; total: number; rows: PlaylistRow[] } };
  /** The crate shown: its terms, joins and first tracks, together, given only once all are in (the last one stays
   *  while the next words are read, so the card is not gone between keystrokes; the owner's no-flash rule). */
  let shown = $state<Shown | null>(null);
  const chips = $derived(shown?.chips ?? null);
  const modes = $derived(shown?.modes ?? {});
  const preview = $derived(shown?.preview ?? null);
  /** Changes made on the card: the newest wins, new words drop them. */
  let changes = 0;
  $effect(() => {
    const c = globalSearch.crate;
    const line = q;
    if (words(line).length < 2 || c === null) return void (shown = null);
    if (c.q !== line) return;
    let gone = false;
    untrack(() => {
      changes++;
      previewOf(c.chips, c.modes).then(
        (p) => !gone && (shown = crateShows(c.chips, p.total) ? { chips: c.chips, modes: c.modes, preview: p } : null),
        () => !gone && (shown = null),
      );
    });
    return () => (gone = true);
  });

  /** A change made on the card (a pill's menu, a word added): the card shows the changed crate once its tracks are in,
   *  the one before until then; a crate left with no terms goes. */
  function change(next: Chip[], m: Modes = modes) {
    const n = ++changes;
    if (!next.length) return void (shown = null);
    previewOf(next, m).then(
      (p) => n === changes && (shown = { chips: next, modes: m, preview: p }),
      (e) => n === changes && toast.fail(e, "It couldn't be changed"),
    );
  }

  const parts = $derived(chips ? sentenceOf(chips, modes) : []);
  /** The terms as pills, in the sentence's order. */
  const order = $derived(parts.flatMap((p) => (p.chip !== undefined ? [p.chip] : [])));
  /** Six covers, a record's once: its first track's (the panel's records' own way; an album's id comes with Cratarr's
   *  own server only, and the card showed none for another). */
  const covers = $derived(preview ? recordFirsts(preview.rows, 6).map((r) => coverUrl({ source: preview.source, id: r.id!, album_id: r.album_id })) : []);
  const likeAt = (i: number | undefined) => i !== undefined && !!chips?.[i] && seedsOf([chips[i]]).length > 0;
  const notAt = (i: number | undefined) => i !== undefined && !!chips?.[i]?.not;

  /** The pill whose menu shows. */
  let at = $state<number | null>(null);
  /** A word being added ("+ word" become its field), and the words that read as nothing. */
  let adding = $state<string | null>(null);
  let missed = $state<string[]>([]);
  async function add(e: KeyboardEvent) {
    if (e.key === 'Escape') return void (adding = null);
    if (e.key !== 'Enter' || !chips || adding === null) return;
    e.preventDefault();
    const line = adding.trim();
    if (!line) return void (adding = null);
    const had = chips;
    const r = readOnto(had, await readLine(line, ask));
    missed = r.missed;
    adding = null;
    if (r.add.length) change([...had, ...r.add], { ...modes, ...r.modes });
  }
  const focusing = (el: HTMLInputElement) => el.focus();

  function grab() {
    if (chips) grabCrate(chips, modes);
  }
  let starting = $state(false);
  async function play() {
    if (!chips) return;
    starting = true;
    try {
      const p = await previewCrate(conditionsOf(chips, undefined, modes), 500, prefs.source || undefined, { without: hiddenLibraries() });
      const ids = p.rows.flatMap((r) => (r.id && !r.missing ? [r.id] : []));
      if (ids.length) await playCollection({ kind: 'tracks', source: p.source, ids }, 'play', nameOf(chips));
    } catch (e) {
      toast.fail(e, "It couldn't be played");
    } finally {
      starting = false;
    }
  }
</script>

{#if shown && chips}
  {@const cs = chips}
  <div class="crate">
    <CoverCard {covers} label="A crate of {q}" onopen={grab} wide>
      <h3 class="sentence">{#each parts as p, k (k)}{#if !p.tight && k}{' '}{/if}{#if p.chip !== undefined}<span class="term" class:like={likeAt(p.chip)} class:not={notAt(p.chip)}>{p.text}</span>{:else}<span class="w">{p.text}</span>{/if}{/each}</h3>
      <!-- Its terms as the week's pills (the owner, 2026-10-31): each opens its menu; "+ word" adds one. -->
      <p class="pills">
        {#each order as i (i)}
          <button class:like={likeAt(i)} class:not={notAt(i)} onclick={() => (at = i)}>{cs[i].reading.label}</button>
        {/each}
        {#if adding === null}
          <button class="more" onclick={() => ((adding = ''), (missed = []))}>+ word</button>
        {:else}
          <input class="more" type="search" aria-label="Another word" bind:value={adding} onkeydown={add} onblur={() => !adding?.trim() && (adding = null)} {@attach focusing} />
        {/if}
      </p>
      {#if missed.length}<p>Nothing found for {missed.join(', ')}</p>{/if}
      <div class="acts">
        <button class="pill play" disabled={starting} onclick={play}><Icon name="play" size={18} />Play</button>
        <button class="pill grab" onclick={grab}>Grab this crate</button>
        <span class="count">{plural(preview?.total ?? 0, 'track')}</span>
      </div>
    </CoverCard>
  </div>
  {#if at !== null && cs[at]}
    {@const i = at}
    <QuickTermSheet
      chip={cs[i]}
      {ask}
      onchange={(c) => change(cs.map((x, k) => (k === i ? c : x)))}
      onremove={() => change(cs.filter((_, k) => k !== i))}
      onclose={() => (at = null)}
    />
  {/if}
{/if}

<style>
  /* Its sentence as the panel writes it (lib/CrateSentence): a term on a line, a Sounds like's in the accent, one left
     out struck through, the words between lighter. */
  .crate {
    margin: 6px 0 10px;
  }

  .sentence {
    line-height: 1.5;
  }

  .term {
    border-bottom: 2px solid var(--border);
  }

  .term.like {
    border-bottom-color: var(--accent);
  }

  .term.not {
    text-decoration: line-through;
    text-decoration-color: color-mix(in srgb, var(--danger-ink) 60%, transparent);
  }

  .w {
    color: var(--muted);
    font-weight: 400;
  }

  .pills button.like {
    border-color: color-mix(in srgb, var(--accent) 70%, var(--border));
  }

  .pills button.not {
    text-decoration: line-through;
  }

  .pills .more {
    border-style: dashed;
  }

  .pills input.more {
    width: 12ch;
    padding: 2px 10px;
    border: 1px dashed var(--border);
    border-radius: 999px;
    background: none;
    color: var(--text);
    font: inherit;
    font-size: 0.85rem;
  }

  .acts {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-top: 12px;
  }

  .count {
    margin-left: auto;
    color: var(--muted);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }

  /* Home's two pills: its moment's Play, a quiet one beside it. */
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 18px;
    border: 0;
    border-radius: 999px;
    font: inherit;
    font-size: 14px;
    white-space: nowrap;
    cursor: pointer;
  }

  .pill:disabled {
    opacity: 0.6;
  }

  .play {
    background: var(--accent);
    color: var(--accent-text);
  }

  .grab {
    background: color-mix(in srgb, var(--text) 10%, transparent);
    color: inherit;
  }
</style>
