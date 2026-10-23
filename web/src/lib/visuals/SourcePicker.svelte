<script lang="ts">
  // Picking a cable's end (decision 0056): its source ('source': a signal of the whole track, one of a band's or all of
  // them, a modulator running or a new one of a type), or the knob it goes into ('knob': a search over them). Used by a
  // knob's "+ Add", a cable's Reroute and the graph's drag into the empty.
  import { BAND_NAMES, type Source } from './cable-actions';
  import type { Module, ModuleType } from './patch';
  import { BAND_SIGNALS, SIGNAL_LABEL, TRACK_SIGNALS } from './routes';

  type KnobPick = { module: string; knob: string };
  let {
    mode,
    types,
    modulators = [],
    knobs = [],
    onpick,
    onclose,
  }: {
    mode: 'source' | 'knob';
    types: Record<string, ModuleType>;
    modulators?: Module[];
    /** The knobs a cable may go into ('knob'), each with its name ("Flame · Hue"). */
    knobs?: (KnobPick & { label: string })[];
    onpick: (pick: Source | KnobPick) => void;
    onclose: () => void;
  } = $props();

  let query = $state('');
  const found = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? knobs.filter((k) => k.label.toLowerCase().includes(q)) : knobs;
  });
  const modulatorTypes = $derived(Object.values(types).filter((t) => t.place === 'modulator'));
  /** The track's signals a cable may come from (Track is the patch's own pulse at a new track; all are). */
  const trackSignals = TRACK_SIGNALS;
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onclose()} />
<div class="picker">
  {#if mode === 'knob'}
    <!-- svelte-ignore a11y_autofocus -->
    <input class="search" type="search" placeholder="Search" aria-label="Search the knobs" bind:value={query} autofocus />
    <div class="list">
      {#each found as k (`${k.module}.${k.knob}`)}
        <button class="item" onclick={() => onpick({ module: k.module, knob: k.knob })}>{k.label}</button>
      {:else}
        <p class="none">None</p>
      {/each}
    </div>
  {:else}
    <h3>Whole track</h3>
    <div class="list">
      {#each trackSignals as s (s)}
        <button class="item" onclick={() => onpick({ signal: s })}>{SIGNAL_LABEL[s]}</button>
      {/each}
    </div>
    <h3>Bands</h3>
    <div class="list">
      {#each BAND_SIGNALS as s (s)}
        <div class="band">
          <button class="item" onclick={() => onpick({ signal: s })} title="All bands">{SIGNAL_LABEL[s]}</button>
          <span class="chips">
            {#each BAND_NAMES as name, b (b)}
              <button class="chip" onclick={() => onpick({ signal: s, band: b })} aria-label="{SIGNAL_LABEL[s]} · {name}">{name}</button>
            {/each}
          </span>
        </div>
      {/each}
    </div>
    {#if modulators.length}
      <h3>Modulators</h3>
      <div class="list">
        {#each modulators as m (m.id)}
          <button class="item" onclick={() => onpick({ modulator: m.id })}>{types[m.type]?.label ?? m.type}</button>
        {/each}
      </div>
    {/if}
    <h3>New modulator</h3>
    <div class="list">
      {#each modulatorTypes as t (t.type)}
        <button class="item add" onclick={() => onpick({ newModulator: t.type })}>+ {t.label}</button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .picker {
    display: grid;
    gap: 6px;
  }

  h3 {
    margin: 8px 0 0;
    font-size: 0.8rem;
    font-weight: 600;
    color: rgb(255 255 255 / 0.6);
  }

  .list {
    display: grid;
    gap: 2px;
  }

  .item {
    min-height: 38px;
    padding: 0 8px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: inherit;
    font: inherit;
    font-size: 0.92rem;
    text-align: left;
    cursor: pointer;
  }

  .item:hover,
  .item:focus-visible,
  .chip:hover,
  .chip:focus-visible {
    background: rgb(255 255 255 / 0.08);
  }

  .item.add {
    color: var(--accent-ink);
  }

  .band {
    display: grid;
    grid-template-columns: 7.5em 1fr;
    align-items: center;
    gap: 4px;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .chip {
    height: 28px;
    padding: 0 8px;
    border: 1px solid rgb(255 255 255 / 0.15);
    border-radius: 14px;
    background: none;
    color: rgb(255 255 255 / 0.8);
    font: inherit;
    font-size: 0.75rem;
    cursor: pointer;
  }

  .search {
    height: 36px;
    padding: 0 10px;
    border: 1px solid rgb(255 255 255 / 0.15);
    border-radius: 8px;
    background: rgb(255 255 255 / 0.05);
    color: inherit;
    font: inherit;
  }

  .none {
    margin: 8px;
    color: rgb(255 255 255 / 0.5);
  }
</style>
