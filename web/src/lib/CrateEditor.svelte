<script lang="ts">
  // A crate's conditions being made or changed (decision 0024), in the crate's panel in its edit mode, where its tracks
  // were (lib/PlaylistView.svelte): the same editor for a new crate and one's own. Its conditions all hold (AND), each
  // can be negated or muted; OR within an axis by choosing several values or a range. They are the Filter sheet's cards
  // (lib/ConditionList, decision 0061), on the whole library's tracks (no Any or All). The controls come from the
  // server's axis list (kind and domain), so a new axis needs nothing here. Below them, the unsaved crate itself
  // (lib/CrateResults, the list a crate being grabbed shows too); its size goes to the panel (`total`), which shows it
  // under the name.
  // One crate, two views (0.13, decision 0088; the owner, 2026-10-31): a Sentence / Conditions switch over the same
  // conditions. The sentence (lib/CrateSentence) says what it can of them (lib/quickCrate chipsOf, named by the server's
  // `names`), the rest as how many more; the conditions are the cards. An edit in either is the crate's conditions, so
  // the other shows it. It opens on the sentence where the sentence can say all of it.
  // The one place a crate is built (0.13; the owner, 2026-10-31: "one place where to show contents or building of a
  // crate, and that is in the panel"): a crate grabbed (a search's card, a menu's Sounds like this) opens here with its
  // sentence as it was read (`sentence`) or its words still to read (`words`); a new one empty, the caret in its field.
  import { untrack } from 'svelte';
  import { getCrateAxes, type Axis, type Condition } from './api';
  import ConditionList from './ConditionList.svelte';
  import CrateResults from './CrateResults.svelte';
  import CrateSentence from './CrateSentence.svelte';
  import { chipsOf, conditionsOf, nameOf, namesOf, type Chip, type Modes, type Named } from './quickCrate';
  import { excludedArtists, loadCrateArtists, setNotInCrate } from './excludedArtists.svelte';
  import Seg from './Seg.svelte';
  import { prefs, type FilterCondition } from './prefs.svelte';
  import { toast } from './toast.svelte';

  let {
    conditions = $bindable(),
    total = $bindable(null),
    names = {},
    crate = null,
    name = '',
    sentence = null,
    words = '',
    named = $bindable(''),
  }: {
    conditions: FilterCondition[];
    total?: number | null;
    /** The server's names for the ids its conditions hold (decision 0088). */
    names?: Readonly<Record<string, Named>>;
    /** The crate's id, one already made: its artists left out by hand show in both views. */
    crate?: string | null;
    /** Its name: a built-in crate's says its Sounds like briefly ("Sounds like your week"). */
    name?: string;
    /** A grabbed crate's terms as they were read, its sentence as it starts. */
    sentence?: { chips: Chip[]; min: number; modes: Modes } | null;
    /** Words to read into its sentence as it opens. */
    words?: string;
    /** The name its sentence gives it (lib/quickCrate nameOf), for a crate not named yet. */
    named?: string;
  } = $props();

  // Its artists left out by hand (Not in this crate, decision 0083): "but not <artist>" in the sentence, a row of them
  // under the cards, one tapped taken back. Their own mechanism; the views only say them.
  $effect(() => {
    if (crate) untrack(() => loadCrateArtists(crate));
  });
  const excluded = $derived(crate ? (excludedArtists.crates[crate] ?? []).map((a) => a.name) : []);
  const takeBack = (artist: string) => crate && setNotInCrate(crate, artist, false).catch((e) => toast.fail(e, `${artist} wasn't taken back`));

  /** Names learnt here: a seed added on a card, a term read in the sentence. */
  let added = $state<Record<string, Named>>({});
  const allNames = $derived({ ...names, ...added });

  const VIEWS = [
    { value: 'sentence', label: 'Sentence' },
    { value: 'conditions', label: 'Conditions' },
  ];
  // The sentence's terms, cut and joins while it shows, and what it cannot say, kept as it is.
  let chips = $state<Chip[]>([]);
  let min = $state(0);
  let modes = $state<Modes>({});
  let rest = $state<Condition[]>([]);
  /** A Sounds like said briefly (a built-in crate's), kept as it is among the conditions. */
  let brief = $state<{ text: string; condition: Condition } | null>(null);
  function toSentence() {
    const r = chipsOf(conditions, allNames, name);
    [chips, min, modes, rest, brief] = [r.chips, r.min, r.modes, r.rest, r.brief];
    view = 'sentence';
  }
  let view = $state<'sentence' | 'conditions'>('conditions');
  untrack(() => {
    if (sentence) [chips, min, modes, view] = [sentence.chips, sentence.min, sentence.modes, 'sentence'];
    else if (!chipsOf(conditions, allNames, name).rest.length) toSentence();
  });
  $effect(() => {
    named = nameOf(chips);
  });
  /** The sentence (lib/CrateSentence): its words read in, the caret put in its field. */
  let said = $state<{ add: (words: string) => Promise<void>; focus: () => void }>();
  let started = false;
  $effect(() => {
    const s = said;
    if (!s || started) return;
    started = true;
    untrack(() => {
      if (words.trim()) void s.add(words);
      else if (!crate && !conditions.length) s.focus();
    });
  });
  // The sentence's edits are the crate's conditions (what it cannot say after them), its terms' names learnt.
  $effect(() => {
    if (view !== 'sentence') return;
    const next = [...conditionsOf(chips, min, modes), ...(brief ? [brief.condition] : []), ...rest];
    untrack(() => {
      const learnt = namesOf(chips);
      if (Object.keys(learnt).some((k) => allNames[k]?.name !== learnt[k].name)) added = { ...added, ...learnt };
      if (JSON.stringify(next) !== JSON.stringify(conditions)) conditions = next;
    });
  });

  // The axes: their kinds and domains in this library.
  let axes = $state<Axis[] | null>(null);
  $effect(() => {
    getCrateAxes(prefs.source || undefined).then(
      (a) => (axes = a),
      (e) => {
        axes ??= [];
        toast.fail(e, "The crate's axes couldn't be loaded");
      },
    );
  });
</script>

<div class="editor">
  <div class="views">
    <Seg label="Show the crate as" options={VIEWS} bind:value={() => view, (v) => (v === 'sentence' ? toSentence() : (view = 'conditions'))} />
  </div>
  {#if view === 'sentence'}
    <div class="sentence">
      <CrateSentence bind:this={said} bind:chips bind:min bind:modes {rest} brief={brief?.text ?? null} {excluded} onexcluded={takeBack} onmore={() => (view = 'conditions')} />
    </div>
  {:else}
    <ConditionList bind:conditions {axes} crate names={allNames} onnamed={(id, n) => (added = { ...added, [id]: n })} />
    {#if excluded.length}
      <p class="left">
        <span class="row-head">Not in this crate</span>
        {#each excluded as artist (artist)}<button aria-label="Take {artist} back into this crate" onclick={() => takeBack(artist)}>{artist}</button>{/each}
      </p>
    {/if}
  {/if}

  <div class="results">
    <CrateResults {conditions} name={name || 'New crate'} bind:total views={crate ?? 'quick'} onnames={(n) => (added = { ...added, ...n })} />
  </div>
</div>

<style>
  .editor {
    padding: 0 12px;
  }

  .views {
    display: flex;
    padding: 4px 0 12px;
  }

  .sentence {
    display: grid;
    gap: 10px;
  }

  /* As the crate's own "Not in this crate" row (lib/PlaylistView .genres). */
  .left {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin: 12px 0 0;
  }

  .left .row-head {
    color: var(--muted);
    font-size: 13px;
  }

  .left button {
    padding: 4px 10px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: none;
    color: inherit;
    font: inherit;
    font-size: 14px;
    text-decoration: line-through;
    cursor: pointer;
  }

  .results {
    display: grid;
    gap: 10px;
    margin: 18px 0 0;
  }
</style>
