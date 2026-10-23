<script lang="ts" module>
  import type { Axis } from './api';

  /** An axis as a list in view has it (decision 0061): its values with how many of the list's items have each, most
   *  first; absent: the whole library's, without counts. */
  export type ListAxis = Axis & { counts?: number[] };
</script>

<script lang="ts">
  // One condition of a crate or a filter, as a card (decision 0061, the owner's option C): folded, one line with its
  // name and what it is set to, Any or All where the list's items hold tracks, a switch that mutes it without removing
  // it (dimmed), and the chevron; unfolded, its whole control: values as chips with their counts, a range (Tempo with
  // its spread, lib/TempoRange), Not, and Remove. The crate editor and the Filter sheet draw the same cards
  // (lib/ConditionList).
  import { slide } from 'svelte/transition';
  import Icon from './Icon.svelte';
  import type { FilterCondition } from './prefs.svelte';
  import Switch from './Switch.svelte';
  import TempoRange from './TempoRange.svelte';

  let {
    condition = $bindable(),
    axis,
    quantified = false,
    open = $bindable(false),
    bpms = null,
    onremove,
  }: {
    condition: FilterCondition;
    axis: ListAxis | undefined;
    /** The list's items hold tracks (albums, artists, playlists): Any or All of them. */
    quantified?: boolean;
    open?: boolean;
    /** The list's tempos, for the Tempo condition's spread; absent: its range as two numbers. */
    bpms?: readonly (number | null | undefined)[] | null;
    onremove: () => void;
  } = $props();

  const on = $derived(condition.enabled !== false);
  const label = $derived(axis?.label ?? condition.axis);
  const QUANTIFIERS = [
    { value: 'any' as const, label: 'Any' },
    { value: 'all' as const, label: 'All' },
  ];
  const quantifier = $derived(condition.quantifier ?? 'any');

  /** What it is set to, in a few words ("Downtempo, Trip Hop", "80–105", "2015 on", "yes"). */
  const summary = $derived.by(() => {
    const v = condition.value;
    let s = '';
    if (v.kind === 'yes') s = 'yes';
    else if (v.kind === 'any_of') s = v.values.length === 0 ? (axis?.kind === 'membership' ? 'any playlist' : 'none chosen') : v.values.length <= 2 ? v.values.join(', ') : `${v.values.slice(0, 2).join(', ')} +${v.values.length - 2}`;
    else {
      const n = (x: number) => String(Math.round(x));
      s = v.min !== null && v.max !== null ? `${n(v.min)}–${n(v.max)}` : v.min !== null ? `${n(v.min)} on` : v.max !== null ? `up to ${n(v.max)}` : 'any';
    }
    return condition.not ? `not ${s}` : s;
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
    condition.value = { kind: 'any_of', values: vs.includes(v) ? vs.filter((x) => x !== v) : [...vs, v] };
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
      <span class="sum" class:not={condition.not}>· {summary}</span>
    </button>
    {#if quantified}
      {#if open}
        <span class="seg" role="radiogroup" aria-label="Any or all of its tracks">
          {#each QUANTIFIERS as q (q.value)}
            <button role="radio" aria-checked={quantifier === q.value} onclick={() => (condition.quantifier = q.value === 'any' ? undefined : q.value)}>{q.label}</button>
          {/each}
        </span>
      {:else}
        <span class="tag" title={quantifier === 'all' ? 'All of its tracks' : 'Any of its tracks'}>{quantifier === 'all' ? 'ALL' : 'ANY'}</span>
      {/if}
    {/if}
    <Switch hideLabel label="Apply {label}" checked={on} onchange={(v) => (condition.enabled = v ? undefined : false)} />
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
            <button class="chip" aria-pressed={c.on} onclick={() => toggle(c.v)}>{c.v}{#if c.n !== null}<span class="n">{c.n.toLocaleString()}</span>{/if}</button>
          {/each}
        </div>
      {:else if axis && condition.value.kind === 'range'}
        {@const r = condition.value}
        {#if axis.key === 'bpm' && bpms}
          <TempoRange {bpms} bind:value={() => tempoRange, (v) => (condition.value = { kind: 'range', min: v?.[0] ?? null, max: v?.[1] ?? null })} />
        {:else}
          <div class="range">
            <input type="number" inputmode="decimal" aria-label="From" placeholder={axis.min === null ? 'from' : String(Math.floor(axis.min))} value={r.min ?? ''} oninput={(e) => (condition.value = { ...r, min: num(e.currentTarget.value) })} />
            <span>–</span>
            <input type="number" inputmode="decimal" aria-label="To" placeholder={axis.max === null ? 'to' : String(Math.ceil(axis.max))} value={r.max ?? ''} oninput={(e) => (condition.value = { ...r, max: num(e.currentTarget.value) })} />
          </div>
        {/if}
      {/if}
      <div class="foot">
        <label class="notbox"><input type="checkbox" checked={condition.not} onchange={(e) => (condition.not = e.currentTarget.checked)} /> Not</label>
        <button class="remove" onclick={onremove}>Remove</button>
      </div>
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
    color: var(--danger, #c33);
  }

  .tag {
    padding: 1px 6px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    color: var(--accent);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.04em;
  }

  /* Any or All, small beside the switch (the folded card's tag is the same size). */
  .seg {
    display: inline-flex;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 8px;
  }

  .seg button {
    padding: 3px 9px;
    border: 0;
    background: none;
    color: var(--muted);
    font: inherit;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .seg button[aria-checked='true'] {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent);
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

  .notbox {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
    font-size: 14px;
    cursor: pointer;
  }

  .remove {
    padding: 6px 10px;
    border: 0;
    border-radius: 999px;
    background: none;
    color: var(--danger, #c33);
    font: inherit;
    font-size: 14px;
    cursor: pointer;
  }
</style>
