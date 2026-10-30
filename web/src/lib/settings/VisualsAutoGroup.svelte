<script lang="ts">
  // The visuals by themselves (0.10; the owner, 2026-10-30; lib/visualsAuto): the switch, and while on how long the
  // screen is left alone first, the snapshot they start from (or as last seen), and whether the look stays or a new
  // one comes each track. In Settings → This device, and without its heading in the sheet a long press on the player's
  // visuals button opens (lib/VisualsAutoSheet), as the info button's Details (lib/DetailsSheet).
  import { slide } from 'svelte/transition';
  import { getVisualsSnapshots, type VisualsSnapshot } from '../api';
  import { reducedMotion } from '../motion';
  import { prefs, VISUALS_AFTER } from '../prefs.svelte';
  import Seg from '../Seg.svelte';
  import { libraryOf } from '../visuals/library';
  import ChoiceSheet from './ChoiceSheet.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';

  let { heading = true }: { heading?: boolean } = $props();

  const afterLabel = (ms: number) => (ms < 60_000 ? `${ms / 1000} seconds` : ms === 60_000 ? '1 minute' : `${ms / 60_000} minutes`);
  const AFTER = VISUALS_AFTER.map((ms) => ({ value: String(ms), label: afterLabel(ms) }));
  const RUN = [
    { value: 'fixed' as const, label: 'Fixed' },
    { value: 'track' as const, label: 'New each track' },
  ];

  // The snapshot library (lib/visuals/library: the saved ones, then the examples), read while it shows.
  let saved = $state<VisualsSnapshot[]>([]);
  $effect(() => {
    if (prefs.visualsAuto) getVisualsSnapshots().then((l) => (saved = l ?? []), () => {});
  });
  const starts = $derived([{ value: 'last', label: 'Last viewed' }, ...libraryOf(saved).map((e) => ({ value: e.id, label: e.label }))]);
  const startLabel = $derived(starts.find((s) => s.value === prefs.visualsAutoStart)?.label ?? 'Last viewed');

  let choosing = $state<'after' | 'start' | null>(null);
</script>

<Group title={heading ? 'Visuals' : null}>
  <Item
    name="Open by themselves"
    line="While music plays and the screen is left alone; a touch closes them"
    toggle={{ checked: prefs.visualsAuto, onchange: (on) => (prefs.visualsAuto = on) }}
  />
  {#if prefs.visualsAuto}
    <div transition:slide={{ duration: reducedMotion() ? 0 : 160 }}>
      <Item name="After" value={afterLabel(prefs.visualsAutoAfter)} onclick={() => (choosing = 'after')} />
      <Item name="Start from" value={startLabel} onclick={() => (choosing = 'start')} />
      <Item name="The look"><Seg label="The look" options={RUN} bind:value={prefs.visualsAutoRun} /></Item>
    </div>
  {/if}
</Group>

{#if choosing === 'after'}
  <ChoiceSheet title="After" options={AFTER} value={String(prefs.visualsAutoAfter)} onchange={(v) => (prefs.visualsAutoAfter = Number(v))} onclose={() => (choosing = null)} />
{:else if choosing === 'start'}
  <ChoiceSheet title="Start from" options={starts} value={prefs.visualsAutoStart} onchange={(v) => (prefs.visualsAutoStart = v)} onclose={() => (choosing = null)} />
{/if}
