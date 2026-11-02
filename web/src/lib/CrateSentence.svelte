<script lang="ts">
  // A crate read as a sentence (0.12, the owner's mockup B; 0.13, decision 0088: one crate, two views): its terms
  // underlined, the words between muted, ending in the field where more is typed ("and…"; the whole line read, a comma
  // or Enter). A term tapped reads another way (lib/QuickTermSheet); a kind's "and" or "or" tapped flips how its terms
  // join (lib/quickCrate joinOf). Where something is "like", the closeness curve (lib/LikeCurve) under it. The panel's
  // alone since 0.13 (the one place a crate is built): the builder's Sentence view (lib/CrateEditor) and a saved crate's
  // head (lib/CrateHead). Its terms, cut and joins are theirs (bound); `rest`, the conditions a sentence cannot say,
  // said as how many more.
  import { untrack } from 'svelte';
  import { likeCurve, search, type Condition } from './api';
  import LikeCurve from './LikeCurve.svelte';
  import { curveKey, type CurveAsk } from './likeCurveAsk';
  import { hiddenLibraries } from './listFilter.svelte';
  import { prefs } from './prefs.svelte';
  import { TERM_KINDS, conditionsOf, countAt, flipped, joinOf, keepsOwn, minAt, readLine, readOnto, seedsOf, sentenceOf, type Chip, type Found, type JoinKind, type Modes } from './quickCrate';
  import QuickTermSheet from './QuickTermSheet.svelte';

  let {
    chips = $bindable(),
    min = $bindable(),
    modes = $bindable(),
    rest = [],
    onmore,
    text = $bindable(''),
    brief = null,
    excluded = [],
    onexcluded,
  }: {
    chips: Chip[];
    min: number;
    modes: Modes;
    /** The crate's conditions a sentence cannot say (lib/quickCrate chipsOf): the curve counts within them too. */
    rest?: readonly Condition[];
    /** A tap on "and N more conditions": they show where they can be changed. */
    onmore?: () => void;
    /** What is typed and not read yet. */
    text?: string;
    /** A Sounds like said in a few words (a built-in crate's, lib/quickCrate chipsOf `brief`: "your week", "Air"). */
    brief?: string | null;
    /** The crate's artists left out by hand (Not in this crate, decision 0083): said "but not <artist>", one tapped
     *  taken back (`onexcluded`). Their own mechanism underneath; the sentence only says them. */
    excluded?: readonly string[];
    onexcluded?: (name: string) => void;
  } = $props();

  /** Words that read as nothing. */
  let missed = $state<string[]>([]);
  /** The term whose sheet shows. */
  let at = $state<number | null>(null);
  let field = $state<HTMLInputElement>();
  /** The caret into its field (a new crate's start). */
  export function focus() {
    field?.focus();
  }

  /** How many of each kind a phrase's readings offer (the owner, 2026-10-30: the right one could not be chosen with one). */
  const EACH = 3;
  const ask = (phrase: string): Promise<Found> =>
    search(phrase, [...TERM_KINDS], EACH).then((r) => TERM_KINDS.flatMap((k) => r.groups[k]?.items.slice(0, EACH) ?? []));

  /** Words read as a whole line (lib/quickCrate readLine), each phrase a term of its best reading, one already in it
   *  once; an "and" or "or" between two terms of one kind sets how they join (modesFrom). A search's words too. */
  export async function add(words: string) {
    if (!words.trim()) return;
    const r = readOnto(chips, await readLine(words, ask));
    missed = r.missed;
    modes = { ...modes, ...r.modes };
    chips = [...chips, ...r.add];
  }

  /** A comma (or a semicolon) ends what is before it: read, the rest stays in the field; Enter reads it all. */
  function typed() {
    const cut = Math.max(text.lastIndexOf(','), text.lastIndexOf(';'));
    if (cut < 0) return;
    void add(text.slice(0, cut));
    text = text.slice(cut + 1).trimStart();
  }
  function entered(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      void add(text);
      text = '';
    } else if (e.key === 'Backspace' && !text && chips.length) {
      // An empty field's Backspace opens the last term (its Remove a tap away), as a token field's would take it.
      at = chips.length - 1;
    }
  }
  const flip = (kind: JoinKind) => (modes = { ...modes, [kind]: flipped(joinOf(modes, kind)) });

  // Nothing read yet: no words before the field, only the example in it, the sentence's own words at work (the owner,
  // 2026-11-02: no "Grab a crate" above it).
  const parts = $derived(chips.length || brief ? sentenceOf(chips, modes, brief) : []);
  const EXAMPLE = 'hip hop and jazz from the 90s like Miles Davis';
  /** The sentence already leaves something out: the crate's own exclusions follow with "not", else "but not". */
  const saysBut = $derived(parts.some((p) => p.text === 'but not'));
  const seeds = $derived(seedsOf(chips));
  const own = $derived(keepsOwn(chips));
  const likedNames = $derived(chips.filter((c) => c.how === 'like' && c.reading.id).map((c) => c.reading.label));

  // The closeness curve, for what is "like", counted among the tracks the other terms (and the rest) leave, as the
  // crate counts them, its join too; asked 300 ms after a change, not again when only its cut moves.
  let curve = $state<number[] | null>(null);
  const within = $derived([...conditionsOf(chips, min, modes), ...rest]);
  const curveAsk = $derived<CurveAsk>({ of: seeds, within, without: hiddenLibraries(), own, ...(joinOf(modes, 'like') === 'or' && seeds.length > 1 ? { mode: 'either' as const } : {}) });
  const curveAt = $derived(curveKey(curveAsk, prefs.source || undefined));
  $effect(() => {
    void curveAt;
    const asked = untrack(() => curveAsk);
    if (!asked.of.length) return void (curve = null);
    let gone = false;
    const t = setTimeout(() => {
      likeCurve(asked, prefs.source || undefined).then(
        (c) => !gone && (curve = c.alike),
        () => !gone && (curve = null),
      );
    }, 300);
    return () => {
      gone = true;
      clearTimeout(t);
    };
  });
</script>

<p class="sentence" class:empty={!chips.length}>
  {#each parts as p, k (k)}{#if k && !p.tight}{' '}{/if}{#if p.chip !== undefined}{@const c = chips[p.chip]}<button
        class="term"
        class:like={!!c?.reading.id && c.how === 'like'}
        class:not={!!c?.not}
        onclick={() => (at = p.chip ?? null)}>{p.text}</button
      >{:else if p.flip}{@const kind = p.flip}<button class="w join" aria-label="Joined by {p.text}: tap for {flipped(p.text as 'and' | 'or')}" onclick={() => flip(kind)}>{p.text}</button
      >{:else}<span class="w">{p.text}</span>{/if}{/each}{#if excluded.length}{#if parts.length}<span class="w">,</span>{' '}{/if}<span class="w"
        >{saysBut || !parts.length ? 'not by' : 'but not by'}</span
      >{#each excluded as name, k (name)}{' '}{#if k}<span class="w">{k === excluded.length - 1 ? 'or' : ','}</span>{' '}{/if}<button
          class="term not"
          aria-label="Take {name} back into this crate"
          onclick={() => onexcluded?.(name)}>{name}</button
        >{/each}{/if}{#if parts.length || excluded.length}{' '}{/if}<input
    type="search"
    placeholder={chips.length ? 'and…' : EXAMPLE}
    aria-label={chips.length ? 'More words for the crate' : 'Words for a crate'}
    bind:this={field}
    bind:value={text}
    oninput={typed}
    onkeydown={entered}
  />
</p>
{#if rest.length}<button class="more" onclick={() => onmore?.()}>and {rest.length} more {rest.length === 1 ? 'condition' : 'conditions'}</button>{/if}
{#if missed.length}<p class="note">Nothing found for {missed.join(', ')}</p>{/if}

{#if chips.length && seeds.length && curve?.length}
  {@const c = curve}
  <div class="near">
    <p class="alike"><span>Alike {likedNames.join(joinOf(modes, 'like') === 'or' ? ' or ' : ' and ')}</span><b>at least {Math.round(min)}%</b></p>
    <LikeCurve curve={c} count={countAt(c, min)} onchange={(n) => (min = minAt(c, n))} />
  </div>
{/if}

{#if at !== null && chips[at]}
  {@const i = at}
  <QuickTermSheet
    chip={chips[i]}
    {ask}
    onchange={(c: Chip) => (chips = chips.map((x, k) => (k === i ? c : x)))}
    onremove={() => (chips = chips.filter((_, k) => k !== i))}
    onclose={() => (at = null)}
  />
{/if}

<style>
  /* The crate as a sentence (mockup B): its terms in the text's own weight, underlined, the words between muted. */
  /* On an accent line, like writing (the owner's 3a, 2026-10-30): no box, before typing or after. */
  .sentence {
    margin: 0;
    padding: 4px 2px 8px;
    border-bottom: 2px solid var(--accent);
    font-size: 22px;
    font-weight: 600;
    line-height: 1.55;
    text-wrap: pretty;
  }

  /* Keyboard focus shows on the line itself, the field having no box of its own. */
  .sentence:focus-within {
    border-bottom-width: 3px;
    padding-bottom: 7px;
  }

  .w {
    color: var(--muted);
    font-weight: 400;
  }

  .term {
    padding: 0 0 1px;
    border: 0;
    border-bottom: 2px solid var(--border);
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  .term.like {
    border-bottom-color: var(--accent);
  }

  /* Left out ("but not"): struck through lightly, as a crossed-out word. */
  .term.not {
    text-decoration: line-through;
    text-decoration-color: color-mix(in srgb, var(--danger-ink) 60%, transparent);
  }

  /* A kind's "and" or "or": muted as the words between, dotted as something a tap changes. */
  .join {
    padding: 0;
    border: 0;
    border-bottom: 1px dotted var(--muted);
    background: none;
    font: inherit;
    font-weight: 400;
    cursor: pointer;
  }

  .term:focus-visible,
  .join:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
    border-radius: 3px;
  }

  /* Where more is typed: the sentence's end, as wide as what is in it. */
  input[type='search'] {
    field-sizing: content;
    min-width: 5ch;
    max-width: 100%;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    font-weight: 400;
    caret-color: var(--accent);
  }

  input[type='search']::placeholder {
    color: var(--muted);
    opacity: 1;
  }

  input[type='search']:focus {
    outline: none;
  }

  /* Nothing typed yet: the example alone, the field as wide as it. */
  .empty input[type='search'] {
    min-width: min(100%, 12ch);
  }

  .more {
    justify-self: start;
    padding: 0;
    border: 0;
    background: none;
    color: var(--muted);
    font: inherit;
    font-size: 14px;
    text-decoration: underline dotted;
    cursor: pointer;
  }

  .near {
    display: grid;
    gap: 4px;
  }

  .alike {
    display: flex;
    justify-content: space-between;
    margin: 0;
    color: var(--muted);
    font-size: 13px;
  }

  .alike b {
    color: var(--text);
    font-weight: 600;
  }

  .note {
    margin: 0;
    color: var(--muted);
    font-size: 13px;
  }
</style>
