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
  import { untrack } from 'svelte';
  import { slide } from 'svelte/transition';
  import Icon from './Icon.svelte';
  import type { FilterCondition } from './prefs.svelte';
  import Switch from './Switch.svelte';
  import DensityRange from './DensityRange.svelte';
  import { trimScale } from './rangeScale';
  import LikeCurve from './LikeCurve.svelte';
  import { likeCurve, type Condition } from './api';
  import { curveKey, type CurveAsk } from './likeCurveAsk';
  import { hiddenLibraries } from './listFilter.svelte';
  import TempoRange from './TempoRange.svelte';
  import { WORD_AXES, wordShown } from './facts';
  import { changed } from './filterRow';
  import { countAt, minAt, SEEDS_LISTED, type Named } from './quickCrate';
  import Seg from './Seg.svelte';
  import { search } from './api';
  import { layerIn, typesSaid } from './recordTypeRules';

  let {
    condition,
    axis,
    known = true,
    items = null,
    open = $bindable(false),
    bpms = null,
    within = null,
    names = {},
    onnamed,
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
    /** What the list's items are when they hold tracks ("record", "playlist"): the condition holds for any or all of
     *  each one's tracks. */
    items?: string | null;
    open?: boolean;
    /** The list's tempos, for the Tempo condition's spread; absent: its range as two numbers. */
    bpms?: readonly (number | null | undefined)[] | null;
    /** A crate's other conditions, a Sounds like's curve counting only the tracks they leave, as the crate picks (0.11,
     *  the owner: "Filtered curve in 0.11"); null: the whole library (a filter's, where the list narrows it too). */
    within?: readonly Condition[] | null;
    /** The names of the ids its conditions hold (Sounds like's seeds; the server's, decision 0088). */
    names?: Readonly<Record<string, Named>>;
    /** A seed added here: its name, for the crate's names. */
    onnamed?: (id: string, named: Named) => void;
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
  // Asked again only when what it counts changes, not when its cut moves (lib/likeCurveAsk curveKey).
  const curveAsk = $derived.by((): CurveAsk | null => {
    const v = condition.value;
    return v.kind === 'like' ? { of: v.of, within: within ?? [], without: hiddenLibraries(), fresh: !!v.fresh, weights: v.weights, own: !!v.own, mode: v.mode } : null;
  });
  const curveAt = $derived(curveAsk && curveKey(curveAsk));
  $effect(() => {
    void curveAt;
    const ask = untrack(() => curveAsk);
    if (!ask || !open) return;
    let gone = false;
    likeCurve(ask).then(
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
    // Cut by how alike (a quick crate's, 0.11): its floor, the handle's while dragged.
    else if (v.kind === 'like' && v.min !== undefined) s = `at least ${dragged !== null && curve ? minAt(curve, dragged) : v.min}% alike`;
    else if (v.kind === 'like') s = `the closest ${dragged ?? v.count}`;
    // The Library's types (0.13): or within the release types, and across (lib/recordTypeRules).
    else if (v.kind === 'any_of' && axis?.key === 'type' && v.values.length) s = typesSaid(v.values, layerIn(axis), shown);
    else if (v.kind === 'any_of' || v.kind === 'all_of') {
      // All of them (0.13): joined "and", as the sentence says it.
      const sep = v.kind === 'all_of' ? ' and ' : ', ';
      s = v.values.length === 0 ? (axis?.kind === 'membership' ? 'any playlist' : 'none chosen') : v.values.length <= 2 ? v.values.map(shown).join(sep) : `${v.values.slice(0, 2).map(shown).join(sep)} +${v.values.length - 2}`;
    }
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
    if (!axis || (condition.value.kind !== 'any_of' && condition.value.kind !== 'all_of')) return [];
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
    const cv = condition.value;
    if (cv.kind !== 'any_of' && cv.kind !== 'all_of') return;
    const vs = cv.values;
    change({ value: { kind: cv.kind, values: vs.includes(v) ? vs.filter((x) => x !== v) : [...vs, v] } });
  }

  // And or or (0.13, decision 0088): a Sounds like's seeds blended or any one of them; a kind a track carries several of
  // (genre, style, mood, tag), any of the values or all of them. Two choices, a slim track.
  const JOINS = [
    { value: 'and', label: 'and' },
    { value: 'or', label: 'or' },
  ];
  const MULTI_AXES = ['genre', 'style', 'mood', 'tag'];
  const joins = $derived(MULTI_AXES.includes(condition.axis));

  // Sounds like's seeds by name (0.13: they were ids nobody saw), one taken out, one added by search.
  const gone = (id: string) => `${id.startsWith('al-') ? 'an album' : id.startsWith('tr-') ? 'a track' : 'an artist'} no longer in the library`;
  const seedName = (id: string) => names[id]?.name ?? gone(id);
  let seedFind = $state('');
  /** Many seeds, or weighted (a built-in crate's, Sounds like your week): summed up, listed on a tap. */
  let seedsShown = $state(false);
  let seedHits = $state<{ id: string; named: Named }[]>([]);
  $effect(() => {
    const q = seedFind.trim();
    if (q.length < 2) return void (seedHits = []);
    let stale = false;
    const t = setTimeout(() => {
      search(q, ['artist', 'album', 'track'], 4, 0, hiddenLibraries()).then(
        (r) => {
          if (stale) return;
          const hits: { id: string; named: Named }[] = [];
          for (const it of r.groups.artist?.items ?? []) if ((it as { id?: string }).id) hits.push({ id: (it as { id?: string }).id!, named: { name: it.title, kind: 'artist' } });
          for (const it of r.groups.album?.items ?? []) {
            const id = /^album:[^/]+\/(.+)$/.exec(it.subject)?.[1];
            if (id) hits.push({ id, named: { name: it.title, kind: 'album' } });
          }
          for (const it of r.groups.track?.items ?? []) {
            const id = (it as { track?: string }).track;
            if (id) hits.push({ id, named: { name: it.title, kind: 'track' } });
          }
          seedHits = hits;
        },
        () => !stale && (seedHits = []),
      );
    }, 250);
    return () => {
      stale = true;
      clearTimeout(t);
    };
  });
  function addSeed(id: string, named: Named) {
    const v = condition.value;
    if (v.kind !== 'like' || v.of.includes(id)) return;
    onnamed?.(id, named);
    change({ value: { ...v, of: [...v.of, id] } });
    seedFind = '';
    seedHits = [];
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
      {#if axis && (condition.value.kind === 'any_of' || condition.value.kind === 'all_of')}
        {@const cv = condition.value}
        {#if joins && cv.values.length > 1}
          <div class="join">
            <Seg label="Any or all of them" options={JOINS} bind:value={() => (cv.kind === 'all_of' ? 'and' : 'or'), (j) => change({ value: { kind: j === 'and' ? 'all_of' : 'any_of', values: cv.values } })} />
          </div>
        {/if}
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
        <!-- Who or what it sounds like, by name (0.13): one taken out, one added; how they join; their own tracks. -->
        {@const many = l.of.length > SEEDS_LISTED || !!l.weights?.length}
        {#if many && !seedsShown}
          <p class="seedsum">
            {l.of.length.toLocaleString()} {l.of.length === 1 ? 'track' : 'tracks'}{l.weights?.length && l.fresh ? ' you played' : ''}
            <button class="showall" onclick={() => (seedsShown = true)}>Show them</button>
          </p>
        {:else}
        <div class="chips seeds">
          {#each l.of as id (id)}
            <button class="chip" aria-pressed="true" disabled={l.of.length < 2} aria-label="Take {seedName(id)} out" onclick={() => change({ value: { ...l, of: l.of.filter((x) => x !== id) } })}
              >{seedName(id)}{#if l.of.length > 1}<Icon name="close" size={14} />{/if}</button
            >
          {/each}
        </div>
        {/if}
        <input class="find" type="search" placeholder="Add an artist, album or track" aria-label="Add to what it sounds like" bind:value={seedFind} />
        {#if seedHits.length}
          <div class="chips">
            {#each seedHits as h (h.id)}
              <button class="chip" onclick={() => addSeed(h.id, h.named)}>{h.named.name}<span class="n">{h.named.kind}</span></button>
            {/each}
          </div>
        {/if}
        {#if l.of.length > 1}
          <div class="join">
            <Seg label="Blended or any of them" options={JOINS} bind:value={() => (l.mode === 'either' ? 'or' : 'and'), (j) => change({ value: { ...l, mode: j === 'or' ? 'either' : undefined } })} />
          </div>
        {/if}
        <div class="own"><Switch label="Their own tracks" checked={!!l.own} onchange={(v) => change({ value: { ...l, own: v || undefined } })} /></div>
        <!-- How many: the curve of how alike the closest are, cut where the crate ends; one cut by how alike (a quick
             crate's) moves its floor to where the handle is let go. -->
        {#if curve?.length}
          {@const c = curve}
          <div class="near">
            <LikeCurve
              curve={c}
              count={l.min !== undefined ? countAt(c, l.min) : l.count}
              onpreview={(n) => (dragged = n)}
              onchange={(n) => change({ value: l.min !== undefined ? { ...l, min: minAt(c, n) } : { ...l, count: n } })}
            />
          </div>
        {/if}
      {:else if axis && condition.value.kind === 'range'}
        {@const r = condition.value}
        {#if axis.bins?.length && axis.min !== null && axis.max !== null && axis.max > axis.min}
          <!-- The profile's bar as the slider: the density over where the tracks are (lib/rangeScale: thin tails cut off,
               an end handle open), the chosen part lit. -->
          {@const sc = trimScale(axis.bins, axis.min, axis.max)}
          <DensityRange
            bins={sc.bins}
            min={Math.floor(sc.min)}
            max={Math.ceil(sc.max)}
            value={{ min: r.min, max: r.max }}
            {label}
            ends={ENDS[axis.key] ?? [`${Math.floor(sc.min)}`, `${Math.ceil(sc.max)}${unit(axis.key)}`]}
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

  .join,
  .own {
    padding: 8px 0 0;
  }

  .seedsum {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin: 4px 0;
    color: var(--muted);
    font-size: 14px;
  }

  .showall {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-ink);
    font: inherit;
    cursor: pointer;
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
