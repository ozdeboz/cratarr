<script lang="ts">
  // A list's profile on top of it (the owner, 2026-10-23; the server's crates::Profile): its telling styles and artists,
  // each number as one bar (lib/DensityBar) on the same scale for every list, and its moods against the library's.
  // Folded or open for every list at once (prefs.profileOpen); folded, it keeps the row chosen in Settings (Display ›
  // AudioMuse), and rows switched off there never show. A row with nothing to say is left out by the server (a number
  // all its tracks share) or here (no styles, no artists).
  import type { ListProfileData } from './api';
  import DensityBar from './DensityBar.svelte';
  import Icon from './Icon.svelte';
  import { PROFILE_ROWS, prefs, type ProfileRow } from './prefs.svelte';

  let { data }: { data: ListProfileData } = $props();

  const ENDS: Record<string, [string, string]> = { tempo: ['60', '180 BPM'], energy: ['calm', 'intense'], danceability: ['still', 'danceable'] };

  /** The rows it has, in their order, less those switched off. */
  const rows = $derived(
    PROFILE_ROWS.filter((r) => {
      if (prefs.profileHidden.includes(r.id)) return false;
      if (r.id === 'style') return data.styles.length > 0;
      if (r.id === 'artists') return data.artists.length > 0;
      if (r.id === 'mood') return data.moods.length > 0;
      return data.numbers.some((n) => n.key === r.id);
    }),
  );
  const shown = $derived(prefs.profileOpen ? rows : rows.filter((r) => r.id === prefs.profileKeep));
  const number = (id: ProfileRow) => data.numbers.find((n) => n.key === id)!;
</script>

{#if rows.length}
  <section class="profile">
    <button class="head" aria-expanded={prefs.profileOpen} onclick={() => (prefs.profileOpen = !prefs.profileOpen)}>
      <span>Profile</span><Icon name={prefs.profileOpen ? 'chevronUp' : 'chevronDown'} size={20} />
    </button>
    {#each shown as r (r.id)}
      <div class="row" class:words={r.id === 'style' || r.id === 'artists'} class:mood={r.id === 'mood'}>
        <span class="label">{r.label}</span>
        {#if r.id === 'style'}
          <div class="chips">{#each data.styles as s (s)}<span>{s}</span>{/each}</div>
        {:else if r.id === 'artists'}
          <div class="artists">{data.artists.join(' · ')}</div>
        {:else if r.id === 'mood'}
          <!-- Each mood above or below the library's middle. -->
          <div class="moods">
            {#each data.moods as m (m.name)}
              <div class="m">
                <div class="col"><b class:up={m.lift >= 0} style:height="{Math.abs(m.lift) * 50}%"></b></div>
                <small>{m.name}</small>
              </div>
            {/each}
          </div>
        {:else}
          {@const n = number(r.id)}
          <DensityBar bins={n.bins} lo={n.lo} hi={n.hi} q1={n.q1} mid={n.mid} q3={n.q3} ends={ENDS[n.key] ?? [String(Math.round(n.lo)), String(Math.round(n.hi))]} />
        {/if}
      </div>
    {/each}
    {#if prefs.profileOpen && data.heard < data.tracks}<p class="heard">{data.heard.toLocaleString()} of {data.tracks.toLocaleString()} heard by AudioMuse</p>{/if}
  </section>
{/if}

<style>
  .profile {
    width: 100%;
    margin-top: 10px;
    border-top: 1px solid var(--border);
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    min-height: 40px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--muted);
    font: inherit;
    font-size: 13.5px;
    cursor: pointer;
  }

  .row {
    display: grid;
    grid-template-columns: 64px minmax(0, 1fr);
    align-items: center;
    gap: 10px;
    min-height: 44px;
  }

  .label {
    color: var(--muted);
    font-size: 13.5px;
  }

  .row.words,
  .row.mood {
    align-items: start;
    padding: 6px 0;
  }

  .row.words .label,
  .row.mood .label {
    padding-top: 4px;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chips span {
    padding: 3px 10px;
    border: 1px solid var(--border);
    border-radius: 99px;
    color: var(--muted);
    font-size: 13.5px;
  }

  .artists {
    padding-top: 3px;
    font-size: 14px;
  }

  .moods {
    display: flex;
    gap: 4px;
  }

  .m {
    flex: 1;
    min-width: 0;
    text-align: center;
  }

  .m small {
    color: var(--muted);
    font-size: 11px;
  }

  /* The library's middle is the line; above it the accent, below it grey. */
  .col {
    position: relative;
    height: 34px;
  }

  .col::before {
    content: '';
    position: absolute;
    top: 50%;
    right: 0;
    left: 0;
    border-top: 1px solid var(--border);
  }

  .col b {
    position: absolute;
    top: 50%;
    right: 30%;
    left: 30%;
    border-radius: 2px;
    background: color-mix(in srgb, var(--text) 35%, transparent);
  }

  .col b.up {
    top: auto;
    bottom: 50%;
    background: var(--accent);
  }

  .heard {
    margin: 4px 0 0;
    color: var(--muted);
    font-size: 12.5px;
  }
</style>
