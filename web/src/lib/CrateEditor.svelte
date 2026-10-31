<script lang="ts">
  // A crate's conditions being made or changed (decision 0024), in the crate's panel in its edit mode, where its tracks
  // were (lib/PlaylistView.svelte): the same editor for a new crate and one's own. Its conditions all hold (AND), each
  // can be negated or muted; OR within an axis by choosing several values or a range. They are the Filter sheet's cards
  // (lib/ConditionList, decision 0061), on the whole library's tracks (no Any or All). The controls come from the
  // server's axis list (kind and domain), so a new axis needs nothing here. Below them, the unsaved crate's first tracks
  // (after 300 ms); its size goes to the panel (`total`), which shows it under the name.
  // One crate, two views (0.13, decision 0088; the owner, 2026-10-31): a Sentence / Conditions switch over the same
  // conditions. The sentence (lib/CrateSentence) says what it can of them (lib/quickCrate chipsOf, named by the server's
  // `names`), the rest as how many more; the conditions are the cards. An edit in either is the crate's conditions, so
  // the other shows it. It opens on the sentence where the sentence can say all of it.
  import { untrack } from 'svelte';
  import { coverUrl, getCrateAxes, previewCrate, type Axis, type Condition, type PlaylistRow } from './api';
  import ConditionList from './ConditionList.svelte';
  import CrateSentence from './CrateSentence.svelte';
  import { chipsOf, conditionsOf, type Chip, type Modes, type Named } from './quickCrate';
  import Seg from './Seg.svelte';
  import { hiddenLibraries } from './listFilter.svelte';
  import { prefs, type FilterCondition } from './prefs.svelte';
  import { toast } from './toast.svelte';
  import Row from './Row.svelte';
  import TrackCover from './TrackCover.svelte';

  let {
    conditions = $bindable(),
    total = $bindable(null),
    names = {},
  }: {
    conditions: FilterCondition[];
    total?: number | null;
    /** The server's names for the ids its conditions hold (decision 0088). */
    names?: Readonly<Record<string, Named>>;
  } = $props();

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
  function toSentence() {
    const r = chipsOf(conditions, allNames);
    [chips, min, modes, rest] = [r.chips, r.min, r.modes, r.rest];
    view = 'sentence';
  }
  let view = $state<'sentence' | 'conditions'>('conditions');
  untrack(() => {
    if (!chipsOf(conditions, allNames).rest.length) toSentence();
  });
  // The sentence's edits are the crate's conditions (what it cannot say after them), its terms' names learnt.
  $effect(() => {
    if (view !== 'sentence') return;
    const next = [...conditionsOf(chips, min, modes), ...rest];
    untrack(() => {
      const learnt: Record<string, Named> = {};
      for (const c of chips)
        if (c.reading.kind === 'album' || c.reading.id) learnt[c.reading.id ?? c.reading.value] = { name: c.reading.label, kind: c.reading.kind === 'album' ? 'album' : c.reading.kind === 'track' ? 'track' : 'artist' };
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
  // The live preview.
  let preview = $state<{ total: number; rows: PlaylistRow[]; source: string } | null>(null);
  let previewError = $state<string | null>(null);
  const key = $derived(JSON.stringify(conditions));
  $effect(() => {
    const k = key;
    const asked = JSON.parse(k) as FilterCondition[];
    const t = setTimeout(() => {
      previewCrate(asked, 50, prefs.source || undefined, { without: hiddenLibraries() }).then(
        (p) => {
          if (k !== key) return;
          preview = p;
          previewError = null;
          total = p.total;
        },
        (e) => {
          if (k !== key) return;
          previewError = e instanceof Error ? e.message : 'Unexpected error';
          total = null;
        },
      );
    }, 300);
    return () => clearTimeout(t);
  });
</script>

<div class="editor">
  <div class="views">
    <Seg label="Show the crate as" options={VIEWS} bind:value={() => view, (v) => (v === 'sentence' ? toSentence() : (view = 'conditions'))} />
  </div>
  {#if view === 'sentence'}
    <div class="sentence"><CrateSentence bind:chips bind:min bind:modes {rest} onmore={() => (view = 'conditions')} /></div>
  {:else}
    <ConditionList bind:conditions {axes} crate names={allNames} onnamed={(id, n) => (added = { ...added, [id]: n })} />
  {/if}

  {#if previewError}
    <p class="error" role="alert">{previewError}</p>
  {:else if preview}
    <ol class="preview" aria-label="The crate's first tracks">
      {#each preview.rows as r, i (i)}
        {@const src = preview.source}
        <li>
          <Row size="large" title={r.name ?? r.file} sub={r.artist ?? ''} off={!r.id || !!r.missing}>
            {#snippet lead()}<TrackCover src={r.id && !r.missing ? coverUrl({ source: src, id: r.id }) : null} />{/snippet}
          </Row>
        </li>
      {/each}
    </ol>
    {#if preview.total > preview.rows.length}<p class="note">and {(preview.total - preview.rows.length).toLocaleString()} more</p>{/if}
  {/if}
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

  .preview {
    margin: 18px -12px 0;
    padding: 0;
    list-style: none;
  }

  .note {
    margin: 4px 0;
    color: var(--muted);
    font-size: 13px;
  }

  .error {
    color: var(--danger-ink);
  }
</style>
