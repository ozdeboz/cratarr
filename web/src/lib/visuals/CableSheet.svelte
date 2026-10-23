<script lang="ts">
  // A cable opened (decision 0056; the owner: a line "clicked normally … you should … see a panel with configuration"),
  // the same in the graph and the rack: where it comes from and goes, how the signal is read (against this track, lately
  // or as it is; fast or slow), whether it swings its knob or lets its setting through, its curve, and Remove. Pure
  // presentation: Visuals.svelte turns a change into the route or the extra cable it is (a route read another way than
  // as routes are becomes an extra cable there).
  import { isRouteId, knobLabel, sourceLabel } from './cable-actions';
  import CurveEditor from './CurveEditor.svelte';
  import { curveOf, valueOf, walksOf, type Curve } from './curves';
  import type { Cable, Module, ModuleType, SignalRef } from './patch';

  let {
    cable,
    types,
    modulators = [],
    modules = [],
    live = null,
    density = null,
    pinned = false,
    onchange,
    onremove,
    onpin,
    onreroute,
  }: {
    cable: Cable;
    types: Record<string, ModuleType>;
    modulators?: Module[];
    /** The patch's modules past the chain (for a knob's name where its module is not a chain one). */
    modules?: Module[];
    /** Where its signal is now and what the curve gives there; how often the signal has been where (CurveEditor's). */
    live?: { x: number; y: number } | null;
    density?: Float32Array | null;
    pinned?: boolean;
    onchange: (next: Cable, done: boolean) => void;
    onremove: () => void;
    onpin?: (on: boolean) => void;
    /** Its source or its knob tapped: to move that end (SourcePicker). */
    onreroute?: (end: 'from' | 'to') => void;
  } = $props();

  const signal = $derived('signal' in cable.from ? (cable.from as SignalRef) : null);
  /** The key is a place on the circle of fifths: only ever as it is. The loudness is as it is unless set otherwise. */
  const onlyAbsolute = $derived(signal?.signal === 'key' && signal.band === undefined);
  const reference = $derived(signal ? (signal.reference ?? (signal.signal === 'loudness' && signal.band === undefined ? 'absolute' : 'track')) : 'track');
  const speed = $derived(signal?.speed ?? 'fast');
  const kind = $derived((types[cable.to.module] ?? types[modules.find((m) => m.id === cable.to.module)?.type ?? ''])?.knobs.find((k) => k.id === cable.to.knob)?.kind);

  const withFrom = (from: Partial<SignalRef>) => onchange({ ...cable, from: { ...(cable.from as SignalRef), ...from } }, true);
  const setCurve = (c: Curve, done: boolean) => onchange({ ...cable, value: valueOf(walksOf(cable.value) ? { ...c, walks: true } : c) }, done);
  const setWalks = (on: boolean) => onchange({ ...cable, value: valueOf({ ...curveOf(cable.value), walks: on || undefined }) }, true);

  const REFERENCES = [
    ['track', 'This track'],
    ['lately', 'Lately'],
    ['absolute', 'As it is'],
  ] as const;
</script>

<div class="sheet">
  <div class="ends">
    <button class="end" disabled={!onreroute} onclick={() => onreroute?.('from')}>{sourceLabel(cable.from, modulators, types)}</button>
    <span class="arrow" aria-hidden="true">→</span>
    <button class="end" disabled={!onreroute} onclick={() => onreroute?.('to')}>{knobLabel(cable.to, types, modules)}</button>
  </div>

  {#if signal && !onlyAbsolute}
    <div class="seg" role="radiogroup" aria-label="Against">
      {#each REFERENCES as [v, name] (v)}
        <button role="radio" aria-checked={reference === v} class:on={reference === v} onclick={() => withFrom({ reference: v })}>{name}</button>
      {/each}
    </div>
    <div class="seg" role="radiogroup" aria-label="Speed">
      {#each [['fast', 'Fast'], ['slow', 'Slow']] as const as [v, name] (v)}
        <button role="radio" aria-checked={speed === v} class:on={speed === v} onclick={() => withFrom({ speed: v })}>{name}</button>
      {/each}
    </div>
  {/if}

  {#if !isRouteId(cable.id) && kind !== 'step'}
    <div class="seg" role="radiogroup" aria-label="How it moves its knob">
      {#each [['swing', 'Swing'], ['level', 'Level']] as const as [v, name] (v)}
        <button role="radio" aria-checked={cable.mode === v} class:on={cable.mode === v} onclick={() => onchange({ ...cable, mode: v }, true)}>{name}</button>
      {/each}
    </div>
  {/if}

  <CurveEditor curve={curveOf(cable.value)} mixed={false} {live} {density} walks={walksOf(cable.value)} {pinned} onchange={setCurve} onwalks={setWalks} onpin={(on) => onpin?.(on)} />

  <button class="remove" onclick={onremove}>Remove</button>
</div>

<style>
  .sheet {
    display: grid;
    gap: 10px;
  }

  .ends {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .end {
    min-width: 0;
    padding: 6px 8px;
    overflow: hidden;
    border: 0;
    border-radius: 8px;
    background: rgb(255 255 255 / 0.06);
    color: inherit;
    font: inherit;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
  }

  .end:disabled {
    background: none;
    cursor: default;
  }

  .arrow {
    color: rgb(255 255 255 / 0.5);
  }

  .seg {
    display: flex;
    gap: 2px;
    padding: 2px;
    border-radius: 10px;
    background: rgb(255 255 255 / 0.06);
  }

  .seg button {
    flex: 1;
    height: 32px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: rgb(255 255 255 / 0.75);
    font: inherit;
    font-size: 0.85rem;
    cursor: pointer;
  }

  .seg button.on {
    background: rgb(255 255 255 / 0.14);
    color: #fff;
  }

  .remove {
    justify-self: start;
    height: 36px;
    padding: 0 12px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--danger-ink);
    font: inherit;
    cursor: pointer;
  }
</style>
