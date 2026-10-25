<script lang="ts">
  // Adjust BPM (the owner, 2026-10-23): one step inside a track's menu or a selection's, an administrator's. A tempo
  // measured at double or half (a hip-hop track at 168 instead of 84) is halved or doubled, or reset to Cratarr's own
  // measure; written into the files' BPM tags, for everyone. Nothing plays any differently. Undo in the toast.
  import { askIfMany } from './many.svelte';
  import { plural } from './insights.svelte';
  import { adjustTempo, getTempo, type TempoAdjust, type TrackTempo } from './api';
  import MarkLoader from './MarkLoader.svelte';
  import { refresh } from './live.svelte';
  import { toast } from './toast.svelte';
  import { withUndo } from './undo.svelte';

  let { source, tracks }: { source: string; tracks: { id: string; name: string }[] } = $props();

  let tempo = $state<TrackTempo[] | null>(null);
  let failed = $state(false);
  let working = $state(false);
  const ids = $derived(tracks.map((t) => t.id));
  function load() {
    getTempo(source, ids).then(
      (r) => (tempo = r.tracks),
      () => (failed = true),
    );
  }
  $effect(load);

  const one = $derived(tempo && tracks.length === 1 ? (tempo[0] ?? null) : null);
  const names = $derived(new Map(tracks.map((t) => [t.id, t.name])));
  /** Reset where a track's tag says other than Cratarr's measure. */
  const resettable = $derived((tempo ?? []).some((t) => t.measured !== null && t.bpm !== t.measured));
  const known = $derived((tempo ?? []).some((t) => t.bpm !== null));

  const WORDS: Record<TempoAdjust, string> = { half: 'Halved.', double: 'Doubled.', reset: 'Reset.' };
  // Many ask first (lib/many).
  function adjust(how: TempoAdjust) {
    const n = plural(ids.length, 'track');
    askIfMany(ids.length, { title: `Change the BPM of ${n}?`, label: `Change ${n}`, icon: 'reset' }, () => void change(how));
  }

  async function change(how: TempoAdjust) {
    working = true;
    try {
      const r = await adjustTempo(source, ids, how);
      tempo = r.tracks;
      withUndo(one && r.tracks[0]?.bpm ? `${r.tracks[0].bpm} BPM.` : WORDS[how], [r.undo], { after: load });
      refresh();
    } catch (e) {
      toast.fail(e, "The BPM didn't change");
    } finally {
      working = false;
    }
  }
</script>

{#if tempo}
  <div class="tempo">
    {#if one}
      <p class="now">{one.bpm !== null ? `${one.bpm} BPM` : 'No BPM'}</p>
    {:else}
      <ul>
        {#each tempo as t (t.id)}
          <li><span class="name">{names.get(t.id) ?? t.id}</span><span class="bpm">{t.bpm ?? '–'}</span></li>
        {/each}
      </ul>
    {/if}
    {#if known}
      <div class="choices">
        <button disabled={working} onclick={() => adjust('half')}>÷2{#if one?.bpm} · {Math.round(one.bpm / 2)}{/if}</button>
        <button disabled={working} onclick={() => adjust('double')}>×2{#if one?.bpm} · {one.bpm * 2}{/if}</button>
        {#if resettable}
          <button class="wide" disabled={working} onclick={() => adjust('reset')}>Reset{#if one?.measured} · {one.measured}{/if}</button>
        {/if}
      </div>
    {/if}
  </div>
{:else if failed}
  <p class="failed">The tempo can't be read.</p>
{:else}
  <div class="loading"><MarkLoader size={40} /></div>
{/if}

<style>
  .tempo {
    padding: 0 16px 16px;
  }

  .now {
    margin: 4px 0 14px;
    font-size: 1.9rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  ul {
    margin: 0 0 14px;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 7px 0;
    border-bottom: 1px solid var(--border);
    font-size: 0.9rem;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .bpm {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .choices {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  .choices button {
    padding: 12px 16px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: color-mix(in srgb, var(--text) 6%, transparent);
    color: var(--text);
    font: inherit;
    font-size: 1.05rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .choices button:disabled {
    opacity: 0.5;
  }

  .choices .wide {
    grid-column: 1 / -1;
  }

  .failed {
    padding: 0 16px 12px;
    color: var(--danger-ink);
  }

  .loading {
    display: grid;
    place-items: center;
    padding: 16px 0;
  }
</style>
