<script lang="ts">
  // A list's profile on top of it (the owner, 2026-10-23; the server's crates::Profile): its telling styles and artists,
  // each number as one bar (lib/DensityBar) on the same scale for every list, and its moods against the library's.
  // Folded or open for every list at once (prefs.profileOpen), by a pill on a divider after its rows (the owner's
  // choice, 2026-10-23); folded, it keeps the rows above the fold line set in Settings (Display › AudioMuse), and rows
  // switched off there never show. A row with nothing to say is left out by the server (a number
  // all its tracks share) or here (no styles, no artists).
  import type { ListProfileData } from './api';
  import DensityBar from './DensityBar.svelte';
  import Icon from './Icon.svelte';
  import { PROFILE_ROWS, prefs, type ProfileRow } from './prefs.svelte';

  /** `ownArtist`: an album's or an artist's own list, whose artists the header already names. */
  let { data, ownArtist = false }: { data: ListProfileData; ownArtist?: boolean } = $props();

  const ENDS: Record<string, [string, string]> = { tempo: ['60', '180 BPM'], energy: ['calm', 'intense'], danceability: ['still', 'danceable'] };

  /** The rows switched on, in the user's order, that it has something for. */
  const rows = $derived(
    prefs.profileRows.flatMap((id) => PROFILE_ROWS.filter((r) => r.id === id)).filter((r) => {
      if (r.id === 'style') return data.styles.length > 0;
      if (r.id === 'artists') return !ownArtist && data.artists.length > 0;
      if (r.id === 'mood') return data.moods.length > 0;
      return data.numbers.some((n) => n.key === r.id);
    }),
  );
  /** The rows above the fold line (by the user's order, whatever this list has of them). */
  const kept = $derived(rows.filter((r) => prefs.profileRows.indexOf(r.id) < prefs.profileFoldAt));
  const shown = $derived(prefs.profileOpen ? rows : kept);
  /** Folding changes something: some row is below the line. */
  const folds = $derived(kept.length < rows.length);
  const number = (id: ProfileRow) => data.numbers.find((n) => n.key === id)!;
</script>

{#if rows.length}
  <section class="profile">
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
    {#if folds}
      <div class="fold">
        <button class="pill" aria-expanded={prefs.profileOpen} onclick={() => (prefs.profileOpen = !prefs.profileOpen)}
          ><Icon name={prefs.profileOpen ? 'chevronUp' : 'chevronDown'} size={16} /><span>{prefs.profileOpen ? 'Fold' : 'Profile'}</span></button
        >
      </div>
    {/if}
  </section>
{/if}

<style>
  .profile {
    width: 100%;
    margin-top: 10px;
  }

  /* The fold control: a pill on a divider after the rows (between the profile and the list). */
  .fold {
    position: relative;
    display: flex;
    justify-content: center;
    margin: 6px 0 0;
  }

  .fold::before {
    content: '';
    position: absolute;
    top: 50%;
    right: 0;
    left: 0;
    border-top: 1px solid var(--border);
  }

  .pill {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    min-height: 32px;
    padding: 0 14px;
    border: 1px solid var(--border);
    border-radius: 99px;
    background: var(--surface);
    color: var(--muted);
    font: inherit;
    font-size: 13px;
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
