<script lang="ts">
  // A playlist as a row, wherever playlists are listed (docs/ui-conventions.md → Rows): its name, then its marks (pinned,
  // shared with everyone), under it its sync while copies are still being made, and at the end its count with its flags (a favourite's small heart above the
  // problems' dot, in the dot's place, so the row gets no wider). Medium on the Playlists page, small in the Export tab,
  // large in the Info and Repair tabs; what the row's action is belongs to where it is. Another user's shared playlist
  // (decision 0051) says whose it is under its name; one's own shared with the others has the shared mark.
  import type { Snippet } from 'svelte';
  import type { Attachment } from 'svelte/attachments';
  import type { Playlist } from './api';
  import { isPlaylistFavorite } from './favourites.svelte';
  import Icon from './Icon.svelte';
  import Meter from './Meter.svelte';
  import { prefs } from './prefs.svelte';
  import Row, { type Cell } from './Row.svelte';

  let {
    p,
    size,
    sub,
    action,
    onclick,
    label,
    active = false,
    strong = false,
    wrap = false,
    class: extra = '',
    attach,
    cells,
  }: {
    /** As the playlists list has it; only the name is needed for one that is not listed (yet). */
    p: Pick<Playlist, 'id' | 'name'> & Partial<Playlist>;
    size: 'small' | 'medium' | 'large';
    sub?: string | Snippet;
    action?: Snippet;
    onclick?: () => void;
    label?: string;
    active?: boolean;
    strong?: boolean;
    /** Its action may go under it when narrow (a large row's text button). */
    wrap?: boolean;
    class?: string;
    attach?: Attachment<HTMLElement>;
    /** Its columns in a table (decision 0042, lib/Row). */
    cells?: readonly Cell[];
  } = $props();

  const missing = (n: number) => `${n} missing ${n === 1 ? 'track' : 'tracks'}`;
  const shared = $derived((p.links ?? []).some((l) => l.target === 'shared'));
  // Its copies in the accounts: shown while some are still to be made (a few per pass).
  const copies = $derived((p.links ?? []).filter((l) => l.target !== 'shared'));
  const made = $derived(copies.filter((l) => l.made).length);
  const syncing = $derived(made < copies.length);
  const fav = $derived(isPlaylistFavorite(p.id));
  const dead = $derived(p.dead ?? 0);
</script>

<Row
  {size}
  title={p.name}
  sub={sub ?? (syncing ? syncMeter : p.mine === false ? p.owner : undefined)}
  {action}
  {onclick}
  {label}
  {active}
  {strong}
  {wrap}
  class={extra}
  {attach}
  tooltip={dead > 0 ? missing(dead) : undefined}
  count={p.count}
  {cells}
>
  {#snippet badges()}
    {#if prefs.pinned.includes(p.id)}<span class="mark" title="Pinned"><Icon name="pin" size={13} /></span>{/if}
    {#if shared}<span class="mark" title="Shared with everyone"><Icon name="users" size={13} /></span>
    {:else if p.shared && p.mine !== false}<span class="mark" title="Shared with other users"><Icon name="users" size={13} /></span>{/if}
  {/snippet}
  {#snippet meta()}
    {#if fav || dead > 0}
      <span class="flags">
        {#if fav}<span class="heart" aria-label="Favourite"><Icon name="heart" size={10} /></span>{/if}
        {#if dead > 0}<i class="dead" aria-label={missing(dead)}></i>{/if}
      </span>
    {/if}
  {/snippet}
</Row>

{#snippet syncMeter()}
  <Meter label={made === 0 ? 'Waiting to sync' : `Syncing ${made} of ${copies.length}`} value={made / copies.length} />
{/snippet}

<style>
  /* Shared with everyone (decision 0014), or pinned. */
  .mark {
    display: flex;
    flex: none;
    opacity: 0.65;
  }

  .flags {
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    width: 10px;
    margin-left: -4px;
  }

  .heart {
    display: flex;
    color: #e5484d;
  }

  .heart :global(svg) {
    fill: currentColor;
  }

  .dead {
    display: block;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--warn);
  }
</style>
