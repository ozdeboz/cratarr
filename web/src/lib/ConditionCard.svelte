<script lang="ts" module>
  import type { Axis } from './api';

  /** An axis as a list in view has it (decision 0061): its values with how many of the list's items have each, most
   *  first; absent: the whole library's, without counts. */
  export type ListAxis = Axis & { counts?: number[] };
</script>

<script lang="ts">
  // One condition of a crate or a filter, as a card (decision 0061, the owner's option C): folded, one line with its
  // name and what it is set to, a switch that mutes it without removing it (dimmed), and the chevron; unfolded, its
  // whole control: values as chips with their counts, a range (Tempo with its spread, lib/TempoRange), Not (a pill:
  // on, it and the chosen values turn red), Remove, and last, where the list's items hold tracks, a sentence of its
  // own saying what the condition is tested on: "Matching any | all tracks in each album" (the owner, 2026-10-23: next
  // to the switch, Any and All read as being about the chosen values). The crate editor and the Filter sheet draw the same cards
  // (lib/ConditionList).
  import { slide } from 'svelte/transition';
  import Icon from './Icon.svelte';
  import type { FilterCondition } from './prefs.svelte';
  import Switch from './Switch.svelte';
  import DensityRange from './DensityRange.svelte';
  import LikeCurve from './LikeCurve.svelte';
  import { likeCurve } from './api';
  import { hiddenLibraries } from './listFilter.svelte';
  import TempoRange from './TempoRange.svelte';
  import { WORD_AXES, wordShown } from './facts';
  import { changed } from './filterRow';

  let {
    condition,
    axis,
    known = true,
    items = null,
    open = $bindable(false),
    bpms = null,
    onchange,
    onremove,
  }: {
    condition: FilterCondition;
    /** A change: a new condition in its place, never this one changed (the owner, 2026-10-29: edits made in place went
     *  unseen by the lists, so Not or a chip never asked the server again). */
    onchange: (next: FilterCondition) => void;
    axis: ListAxis | undefined;
    /** The axes have been read: until then no name and no summary, not the raw key ("bpm") and "none chosen" before
     *  "Tempo · 80–105 BPM" (the owner, 2026-10-24). */
    known?: boolean;
    /** What the list's items are when they hold tracks ("album", "playlist"): the condition holds for any or all of
     *  each one's tracks. */
    items?: string | null;
    open?: boolean;
    /** The list's tempos, for the Tempo condition's spread; absent: its range as two numbers. */
    bpms?: readonly (number | null | undefined)[] | null;
    onremove: () => void;
  } = $props();

  const on = $derived(condition.enabled !== false);
  /** This condition with `patch`, a new one in its place (lib/filterRow). */
  const change = (patch: Partial<FilterCondition>) => onchange(changed(condition, patch));
  const label = $derived(axis?.label ?? (known ? condition.axis : ''));
  const QUANTIFIERS = [
    { value: 'any' as const, label: 'any' },
    { value: 'all' as const, label: 'all' },
  ];
  const quantifier = $derived(condition.quantifier ?? 'any');
  /** Any or all of an item's tracks only where they can differ on the axis (not an album's year: its tracks share
   *  it, so both would pick the same albums). */
  const tested = $derived(!!items && !!axis?.varies);
  /** Sounds like's curve, for its seeds (lib/LikeCurve). */
  let curve = $state<number[] | null>(null);
  /** The count under a finger dragging the curve: the summary follows it before the release sets it. */
  let dragged = $state<number | null>(null);
  $effect(() => {
    const v = condition.value;
    if (v.kind !== 'like' || !open) return;
    const of = v.of.join(',');
    const fresh = !!v.fresh;
    const weights = v.weights;
    let gone = false;
    likeCurve(of.split(','), undefined, hiddenLibraries(), fresh, weights).then(
      (c) => !gone && (curve = c.alike),
      () => !gone && (curve = null),
    );
    return () => {
      gone = true;
    };
  });
  /** The scale's ends as the profile names them (lib/ListProfile). */
  const ENDS: Record<string, [string, string]> = { energy: ['calm', 'intense'], danceability: ['still', 'danceable'] };
  /** A range's unit, said after its numbers: the listener's (plays, days ago, stars) and a track's length. */
  const UNITS: Record<string, string> = { bpm: ' BPM', plays: ' plays', played: ' days ago', added: ' days ago', rating: ' ★', length: ' min' };
  const unit = (key: string | undefined) => (key ? (UNITS[key] ?? '') : '');

  /** A value as shown: AudioMuse's words capitalised. */
  const shown = (v: string) => (axis && WORD_AXES.includes(axis.key) ? wordShown(v) : v);

  /** What it is set to, in a few words ("Downtempo, Trip Hop", "80–105", "2015 on", "yes"). */
  const summary = $derived.by(() => {
    const v = condition.value;
    let s = '';
    if (v.kind === 'yes') s = 'yes';
    else if (v.kind === 'like') s = `the closest ${dragged ?? v.count}`;
    else if (v.kind === 'any_of') s = v.values.length === 0 ? (axis?.kind === 'membership' ? 'any playlist' : 'none chosen') : v.values.length <= 2 ? v.values.map(shown).join(', ') : `${v.values.slice(0, 2).map(shown).join(', ')} +${v.values.length - 2}`;
    else {
      const n = (x: number) => String(Math.round(x));
      const u = unit(axis?.key);
      s = v.min !== null && v.max !== null ? `${n(v.min)}–${n(v.max)}${u}` : v.min !== null ? `${n(v.min)}${u} on` : v.max !== null ? `up to ${n(v.max)}${u}` : 'any';
    }
    s = condition.not ? `not ${s}` : s;
    // All is not the default: said folded too.
    return tested && quantifier === 'all' ? `${s}, all tracks` : s;
  });

  // A values axis can have many (genres): a search, and the chosen ones first, then the most common.
  let find = $state('');
  const chips = $derived.by(() => {
    if (!axis || condition.value.kind !== 'any_of') return [];
    const chosen = condition.value.values;
    const q = find.trim().toLowerCase();
    const count = (v: string) => {
      const i = axis.values.indexOf(v);
      return i < 0 ? null : (axis.counts?.[i] ?? null);
    };
    const rest = axis.values.filter((v) => !chosen.includes(v) && (!q || v.toLowerCase().includes(q)));
    return [...chosen, ...rest.slice(0, q ? 60 : 24)].map((v) => ({ v, n: count(v), on: chosen.includes(v) }));
  });
  function toggle(v: string) {
    if (condition.value.kind !== 'any_of') return;
    const vs = condition.value.values;
    change({ value: { kind: 'any_of', values: vs.includes(v) ? vs.filter((x) => x !== v) : [...vs, v] } });
  }
  const num = (s: string) => (s.trim() === '' || Number.isNaN(Number(s)) ? null : Number(s));
  // Tempo's spread sets the range in 5-BPM bands; the whole scale is no range.
  const tempoRange = $derived<[number, number] | null>(
    condition.value.kind === 'range' && condition.value.min !== null && condition.value.max !== null ? [condition.value.min, condition.value.max] : null,
  );
</script>

<li class="card" class:off={!on}>
  <div class="line">
    <button class="name" aria-expanded={open} onclick={() => (open = !open)}>
      <span class="label">{label}</span>
      {#if known}<span class="sum" class:not={condition.not}>· {summary}</span>{/if}
    </button>
    <Switch hideLabel label="Apply {label}" checked={on} onchange={(v) => change({ enabled: v ? undefined : false })} />
    <button class="chev" aria-label={open ? 'Fold' : 'Unfold'} onclick={() => (open = !open)}><Icon name={open ? 'chevronUp' : 'chevronDown'} size={20} /></button>
  </div>
  {#if open}
    <div class="body" transition:slide={{ duration: 160 }}>
      {#if axis && condition.value.kind === 'any_of'}
        {#if axis.values.length > 24}
          <input class="find" type="search" placeholder="Find {label.toLowerCase()}" bind:value={find} />
        {/if}
        <div class="chips">
          {#each chips as c (c.v)}
            <button class="chip" class:not={condition.not} aria-pressed={c.on} onclick={() => toggle(c.v)}>{shown(c.v)}{#if c.n !== null}<span class="n">{c.n.toLocaleString()}</span>{/if}</button>
          {/each}
        </div>
      {:else if condition.value.kind === 'like'}
        {@const l = condition.value}
        <!-- How many: the curve of how alike the closest are, cut where the crate ends. -->
        {#if curve?.length}<div class="near"><LikeCurve {curve} count={l.count} onpreview={(n) => (dragged = n)} onchange={(n) => change({ value: { ...l, count: n } })} /></div>{/if}
      {:else if axis && condition.value.kind === 'range'}
        {@const r = condition.value}
        {#if axis.bins?.length && axis.min !== null && axis.max !== null && axis.max > axis.min}
          <!-- The profile's bar as the slider: the density over the list's span, the chosen part lit. -->
          <DensityRange
            bins={axis.bins}
            min={Math.floor(axis.min)}
            max={Math.ceil(axis.max)}
            value={{ min: r.min, max: r.max }}
            {label}
            ends={ENDS[axis.key] ?? [`${Math.floor(axis.min)}`, `${Math.ceil(axis.max)}${unit(axis.key)}`]}
            onchange={(v) => change({ value: { kind: 'range', ...v } })}
          />
        {:else if axis.key === 'bpm' && bpms}
          <TempoRange {bpms} bind:value={() => tempoRange, (v) => change({ value: { kind: 'range', min: v?.[0] ?? null, max: v?.[1] ?? null } })} />
        {:else}
          <div class="range">
            <input type="number" inputmode="decimal" aria-label="From" placeholder={axis.min === null ? 'from' : String(Math.floor(axis.min))} value={r.min ?? ''} oninput={(e) => change({ value: { ...r, min: num(e.currentTarget.value) } })} />
            <span>–</span>
            <input type="number" inputmode="decimal" aria-label="To" placeholder={axis.max === null ? 'to' : String(Math.ceil(axis.max))} value={r.max ?? ''} oninput={(e) => change({ value: { ...r, max: num(e.currentTarget.value) } })} />
          </div>
        {/if}
      {/if}
      <div class="foot">
        <button class="notpill" aria-pressed={condition.not} onclick={() => change({ not: !condition.not })}>Not</button>
        <button class="remove" onclick={onremove}>Remove</button>
      </div>
      {#if tested}
        <p class="tested">
          Matching
          <span class="seg" role="radiogroup" aria-label="Any or all tracks">
            {#each QUANTIFIERS as q (q.value)}
              <button role="radio" aria-checked={quantifier === q.value} onclick={() => change({ quantifier: q.value === 'any' ? undefined : q.value })}>{q.label}</button>
            {/each}
          </span>
          tracks in each {items}
        </p>
      {/if}
    </div>
  {/if}
</li>

<style>
  /* A card as the sheet's switches are (lib/FilterSheet's --card), on its own elsewhere. */
  .card {
    padding: 6px 6px 6px 16px;
    border-radius: 20px;
    background: var(--card, color-mix(in srgb, var(--text) 8%, transparent));
    list-style: none;
  }

  /* Muted: kept, not applied; dimmed but still within reach. */
  .card.off .name,
  .card.off .body {
    opacity: 0.5;
  }

  .line {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 44px;
  }

  .name {
    display: flex;
    flex: 1;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .label {
    font-weight: 600;
    white-space: nowrap;
  }

  .sum {
    overflow: hidden;
    color: var(--muted);
    font-size: 14px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sum.not {
    color: var(--danger-ink);
  }


  .near {
    padding: 12px 18px 4px 22px;
  }

  /* What the condition is tested on, a sentence under the card's controls; any or all inside it. */
  .tested {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin: 0;
    padding-top: 10px;
    border-top: 1px solid color-mix(in srgb, var(--text) 10%, transparent);
    color: var(--muted);
    font-size: 14px;
  }

  .seg {
    display: inline-flex;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 8px;
  }

  .seg button {
    padding: 3px 10px;
    border: 0;
    background: none;
    color: var(--muted);
    font: inherit;
    cursor: pointer;
  }

  .seg button[aria-checked='true'] {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent-ink);
  }

  .chev {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .body {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 10px;
    padding: 4px 6px 4px 0;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
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

  .chip[aria-pressed='true'] {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 22%, var(--bg));
  }

  .n {
    color: var(--muted);
    font-size: 12px;
  }

  input[type='search'],
  input[type='number'] {
    min-height: 36px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    color: var(--text);
    font: inherit;
    font-size: 15px;
  }

  .range {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .range input {
    width: 96px;
  }

  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  /* Not: a pill that turns red when on (the owner's choice, 2026-10-23), as the chosen chips and the summary do. */
  .notpill {
    padding: 5px 14px;
    border: 1px solid color-mix(in srgb, var(--text) 28%, transparent);
    border-radius: 999px;
    background: none;
    color: var(--muted);
    font: inherit;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }

  .notpill[aria-pressed='true'] {
    border-color: var(--danger);
    background: color-mix(in srgb, var(--danger) 12%, transparent);
    color: var(--danger-ink);
  }

  .chip.not[aria-pressed='true'] {
    border-color: var(--danger);
    background: color-mix(in srgb, var(--danger) 18%, var(--bg));
  }

  .remove {
    padding: 6px 10px;
    border: 0;
    border-radius: 999px;
    background: none;
    color: var(--danger-ink);
    font: inherit;
    font-size: 14px;
    cursor: pointer;
  }
</style>
