<script lang="ts">
  // A quick crate at the top of Crates (0.12; the owner, 2026-10-30: the crate search's mockup B, "B with A's graph"):
  // the crate read as a sentence (lib/CrateSentence), its terms underlined, the words between muted, ending in the field
  // where more is typed. Then the crate itself (lib/CrateResults, a crate's one list: 0.12.1, the owner: "Results
  // replace the list": Crates shows this crate alone meanwhile), its count with Save, Shuffle and Play and its menu (Edit
  // in the crate builder, Start over). Nothing typed: the last few not saved, to come back to. Its state is
  // lib/quickCrates': it stays while the page is left.
  // One crate, two views (0.13, decision 0088; the owner, 2026-10-31: "one start, one list"): New crate starts here, the
  // sentence's field focused, a Sentence / Conditions switch over the same crate. The conditions are the crate builder's
  // cards (lib/ConditionList); what they make is read back as the sentence's terms (lib/quickCrate chipsOf, as typed
  // where the same: keptChips), what it cannot say kept as it is.
  import { onDestroy, untrack } from 'svelte';
  import { createCrate, getCrateAxes, type Axis, type Condition } from './api';
  import ConditionList from './ConditionList.svelte';
  import { CRATE, loadCrates, startCrate } from './crates.svelte';
  import CrateResults from './CrateResults.svelte';
  import CrateSentence from './CrateSentence.svelte';
  import Icon from './Icon.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { viewPlaylist } from './panel.svelte';
  import { leaveOnArrival } from './navStep.svelte';
  import { prefs } from './prefs.svelte';
  import { chipsOf, CLOSE, conditionsOf, keptChips, nameOf, namesOf, recentFor, type Named } from './quickCrate';
  import { forgetQuick, keepQuick, quickCrates } from './quickCrates.svelte';
  import { reach } from './reach.svelte';
  import Row from './Row.svelte';
  import Seg from './Seg.svelte';
  import { toast } from './toast.svelte';
  import { withUndo } from './undo.svelte';

  const chips = $derived(quickCrates.chips);
  const min = $derived(quickCrates.min);
  /** What is typed and not read yet (the unsaved crates under the field go by it). */
  let text = $state('');
  let menuOpen = $state(false);
  let saving = $state(false);
  /** The sentence (lib/CrateSentence): the terms, their cut and joins are quickCrates'. */
  let sentence = $state<{ add: (words: string) => Promise<void>; focus: () => void }>();

  // A search's words (lib/quickCrates openQuickCrate), read as they come.
  $effect(() => {
    const words = quickCrates.words;
    const s = sentence;
    if (!words || !s) return;
    untrack(() => {
      quickCrates.words = '';
      void s.add(words);
    });
  });
  // New crate: the caret in the sentence's field.
  $effect(() => {
    const s = sentence;
    if (!quickCrates.focus || !s) return;
    untrack(() => {
      view = 'sentence';
      s.focus();
    });
  });

  const conditions = $derived([...conditionsOf(chips, min, quickCrates.modes), ...(quickCrates.brief ? [quickCrates.brief.condition] : []), ...quickCrates.rest]);
  const name = $derived(nameOf(chips) || 'New crate');
  /** Being made: its view (and the crate) shows, the crates' list not. */
  const open = $derived(quickCrates.making || chips.length > 0 || conditions.length > 0);

  // Sentence or conditions; the cards are the conditions as they were when it switched, edited there in place.
  const VIEWS = [
    { value: 'sentence', label: 'Sentence' },
    { value: 'conditions', label: 'Conditions' },
  ];
  let view = $state<'sentence' | 'conditions'>('sentence');
  let cards = $state<Condition[]>([]);
  function show(v: 'sentence' | 'conditions') {
    if (v === 'conditions') cards = $state.snapshot(conditions) as Condition[];
    view = v;
  }
  /** Names learnt here: a seed added on a card, the server's with the crate's tracks. */
  let added = $state<Record<string, Named>>({});
  const allNames = $derived({ ...namesOf(chips), ...added });
  // The cards' edits are the crate: read back as its terms, what the sentence cannot say kept.
  $effect(() => {
    if (view !== 'conditions') return;
    const r = chipsOf(cards, allNames);
    untrack(() => {
      const next = keptChips(r.chips, quickCrates.chips);
      if (JSON.stringify(next) !== JSON.stringify(quickCrates.chips)) quickCrates.chips = next;
      if (quickCrates.min !== r.min) quickCrates.min = r.min;
      if (JSON.stringify(r.modes) !== JSON.stringify(quickCrates.modes)) quickCrates.modes = r.modes;
      if (JSON.stringify(r.rest) !== JSON.stringify(quickCrates.rest)) quickCrates.rest = r.rest;
      if (JSON.stringify(r.brief) !== JSON.stringify(quickCrates.brief)) quickCrates.brief = r.brief;
    });
  });
  // The axes: their kinds and domains in this library, once the conditions show.
  let axes = $state<Axis[] | null>(null);
  $effect(() => {
    if (view !== 'conditions' || axes) return;
    getCrateAxes(prefs.source || undefined).then(
      (a) => (axes = a),
      (e) => {
        axes ??= [];
        toast.fail(e, "The crate's axes couldn't be loaded");
      },
    );
  });

  // The empty page is the regular one (the owner, 2026-10-30): the crates made here and not saved show only once typing
  // starts, those the words may mean.
  const recentShown = $derived(open ? [] : recentFor(quickCrates.recent, text));
  /** Gone from the top (saved, edited, started over): nothing being made. */
  function clear() {
    quickCrates.chips = [];
    quickCrates.min = CLOSE;
    quickCrates.modes = {};
    quickCrates.rest = [];
    quickCrates.brief = null;
    quickCrates.making = false;
    text = '';
    view = 'sentence';
  }
  async function save() {
    saving = true;
    try {
      const made = await createCrate(name, conditions);
      forgetQuick(chips);
      withUndo(`Created ${name}.`, [made.undo], { after: () => loadCrates() });
      await loadCrates();
      // Its words stay until the crate shows (lib/quickCrates; the owner, 2026-10-31: never the page between).
      text = '';
      missed = [];
      leaveOnArrival({ quick: true });
      viewPlaylist(CRATE + made.id);
    } catch (e) {
      toast.fail(e, "The crate wasn't saved");
    } finally {
      saving = false;
    }
  }
  /** Into the crate builder, a new crate's draft: its conditions there to change one by one. */
  function edit() {
    menuOpen = false;
    const [n, c] = [name, conditions];
    forgetQuick(chips);
    // As Save: the words stay until the builder shows.
    text = '';
    missed = [];
    leaveOnArrival({ quick: true });
    startCrate(n, c);
  }
  function startOver() {
    menuOpen = false;
    keepQuick({ chips, min, modes: quickCrates.modes });
    clear();
  }
  // Left unsaved (the page left, the browser closed): kept among the last few, and still here on coming back.
  const keep = () => quickCrates.chips.length && keepQuick({ chips: quickCrates.chips, min: quickCrates.min, modes: quickCrates.modes });
  $effect(() => {
    addEventListener('pagehide', keep);
    return () => removeEventListener('pagehide', keep);
  });
  onDestroy(keep);
</script>

<section class="quick" aria-label="Grab a crate">
  <!-- Out of reach (decision 0085): reading words, the preview and Save are all Cratarr's, so one line instead. -->
  {#if reach.away}
  <p class="note">Grabbing a crate needs Cratarr.</p>
  {:else}
  {#if open}
    <div class="views"><Seg label="Show the crate as" options={VIEWS} bind:value={() => view, show} /></div>
  {/if}
  {#if view === 'sentence' || !open}
    <CrateSentence
      bind:this={sentence}
      bind:chips={quickCrates.chips}
      bind:min={quickCrates.min}
      bind:modes={quickCrates.modes}
      bind:text
      rest={quickCrates.rest}
      brief={quickCrates.brief?.text ?? null}
      onmore={() => show('conditions')}
    />
  {:else}
    <ConditionList bind:conditions={cards} {axes} crate names={allNames} onnamed={(id, n) => (added = { ...added, [id]: n })} />
  {/if}

  {#if open}
    <CrateResults {conditions} {name} onnames={(n) => (added = { ...added, ...n })}>
      {#snippet actions()}
        <button class="icon" aria-label="Save as a crate" title="Save" disabled={!conditions.length || saving} onclick={save}><Icon name="save" size={22} /></button>
        <button class="icon" aria-label="The crate's menu" onclick={() => (menuOpen = true)}><Icon name="moreVert" size={22} /></button>
      {/snippet}
    </CrateResults>
  {:else if recentShown.length}
    <ul class="recent" aria-label="Made here, not saved">
      {#each recentShown as k (nameOf(k.chips))}
        <li>
          <Row
            size="medium"
            title={nameOf(k.chips)}
            onclick={() => {
              quickCrates.chips = k.chips.map((c) => ({ ...c }));
              quickCrates.min = k.min;
              quickCrates.modes = { ...(k.modes ?? {}) };
            }}
          >
            {#snippet lead()}<span class="lead"><Icon name="clock" size={20} /></span>{/snippet}
          </Row>
        </li>
      {/each}
    </ul>
  {/if}
  {/if}
</section>

{#if menuOpen}
  <MenuSheet label="The crate being made" icon="crates" title={name} onclose={() => (menuOpen = false)}>
    <MenuEntry icon="pencil" label="Edit" onclick={edit} />
    <MenuEntry icon="close" label="Start over" onclick={startOver} />
  </MenuSheet>
{/if}

<style>
  .quick {
    display: grid;
    gap: 10px;
    margin: 4px 0 14px;
  }

  .views {
    display: flex;
  }

  .recent {
    margin: 0 -8px;
    padding: 0;
    list-style: none;
  }

  .lead {
    display: grid;
    place-items: center;
    width: 40px;
    color: var(--muted);
  }

  .note {
    margin: 0;
    color: var(--muted);
    font-size: 13px;
  }
</style>
