<script lang="ts">
  // An album's fixes on Health's third level (0.9.7; the owner, 2026-10-26: Cratarr proposes, an administrator
  // approves): each field as it is and as it would be, where that comes from and why, a likely one said so; then Apply
  // writes them into the files (Undo puts the old values back), as Renumber's Fix does (lib/HealthRenumber). A genre's
  // genres are picked (lib/GenrePick); those picked are written as its genres.
  import type { Fix } from './api';
  import { apply, FIELD_LABEL, fixesOf, healthFixes, readFixes } from './fixes.svelte';
  import GenrePick from './GenrePick.svelte';
  import Icon from './Icon.svelte';
  import { me } from './me.svelte';
  import PageLoading from './PageLoading.svelte';

  /** `track`: one track's (a length's fix), not the whole album's. */
  let { finding, album, track = null, title, ongone }: { finding: string; album: string; track?: string | null; title: string; ongone: () => void } = $props();

  $effect(() => readFixes(finding));
  const known = $derived(!!healthFixes[finding]);
  const mine = $derived(fixesOf(finding, album).filter((f) => !track || f.id.endsWith(`:${track}`)));
  const matching = $derived(!!healthFixes[finding]?.matching);
  let working = $state(false);

  // The genres picked, per fix (none touched: its strongest).
  let picked = $state<Record<string, string[]>>({});
  const pickedOf = (f: Fix) => picked[f.id] ?? [f.to];
  const nonePicked = $derived(mine.some((f) => f.candidates?.length && !pickedOf(f).length));

  /** A value as it reads: a cover's address is its picture, an empty one a dash. */
  const shown = (f: Fix, v: string | null) => (v === null || v === '' ? '–' : v);

  async function fix() {
    if (working || !mine.length) return;
    working = true;
    const chosen = Object.fromEntries(mine.filter((f) => f.candidates?.length).map((f) => [f.id, pickedOf(f)]));
    if (await apply({ ids: mine.map((f) => f.id), chosen }, finding, `Fixed ${title}`)) ongone();
    working = false;
  }
</script>

{#if !known || (matching && !mine.length)}
  <PageLoading />
{:else if !mine.length}
  <p class="muted">Nothing to propose for it yet.</p>
{:else}
  <ul class="fixes">
    {#each mine as f (f.id)}
      <li>
        <span class="field">{FIELD_LABEL[f.field]}{#if f.certainty === 'likely'}<span class="likely"> · likely</span>{/if}</span>
        {#if f.field === 'cover'}
          <img class="pic" src={f.to} alt="" />
        {:else if f.candidates?.length}
          <GenrePick fix={f} bind:picked={() => pickedOf(f), (v) => (picked[f.id] = v)} />
        {:else}
          <span class="change"><span class="now">{shown(f, f.from)}</span><Icon name="arrowRight" size={14} /><b>{shown(f, f.to)}</b></span>
        {/if}
        <small class="why">{f.source} · {f.why}</small>
      </li>
    {/each}
  </ul>
  {#if me.admin}
    <button class="act" disabled={working || nonePicked} aria-busy={working} onclick={fix}>{mine.length === 1 ? 'Apply' : `Apply ${mine.length}`}</button>
  {/if}
{/if}

<style>
  p {
    margin: 0;
  }

  .muted {
    color: var(--muted);
  }

  .fixes {
    display: grid;
    gap: 14px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .fixes li {
    display: grid;
    gap: 4px;
    min-width: 0;
  }

  .field {
    color: var(--muted);
    font-size: 13px;
  }

  .likely {
    color: var(--warn-ink);
  }

  .change {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    overflow-wrap: anywhere;
  }

  .now {
    color: var(--muted);
  }

  .why {
    color: var(--muted);
  }

  .pic {
    width: 96px;
    height: 96px;
    border-radius: 6px;
    object-fit: cover;
  }

  /* As the panel's own way onward (lib/HealthPanel .act). */
  .act {
    justify-self: start;
    padding: 10px 18px;
    border: 0;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-text);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .act:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
