<script lang="ts">
  // A playlist as a row, wherever playlists are listed (docs/ui-conventions.md → Rows): its name, then its marks (pinned,
  // kept in an account's own playlists), and at the end its count with its flags (a favourite's small heart above the
  // problems' dot, in the dot's place, so the row gets no wider). Medium on the Playlists page, small in the Export tab,
  // large in the Info and Repair tabs; what the row's action is belongs to where it is.
  import type { Snippet } from 'svelte';
  import type { Attachment } from 'svelte/attachments';
  import type { Playlist } from './api';
  import { isPlaylistFavorite } from './favourites.svelte';
  import Icon from './Icon.svelte';
  import { prefs } from './prefs.svelte';
  import Row from './Row.svelte';

  let {
    p,
    size,
    sub,
    action,
    onclick,
    label,
    active = false,
    strong = false,
    class: extra = '',
    attach,
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
    class?: string;
    attach?: Attachment<HTMLElement>;
  } = $props();

  const missing = (n: number) => `${n} missing ${n === 1 ? 'track' : 'tracks'}`;
  const own = $derived((p.links ?? []).filter((l) => l.target !== 'shared'));
  const fav = $derived(isPlaylistFavorite(p.id));
  const dead = $derived(p.dead ?? 0);
</script>

<Row
  {size}
  title={p.name}
  {sub}
  {action}
  {onclick}
  {label}
  {active}
  {strong}
  class={extra}
  {attach}
  tooltip={dead > 0 ? missing(dead) : undefined}
  count={p.count}
>
  {#snippet badges()}
    {#if prefs.pinned.includes(p.id)}<span class="mark" title="Pinned"><Icon name="pin" size={13} /></span>{/if}
    {#if own.length}<span class="mark" title={own.map((l) => l.label).join(', ')}><Icon name="user" size={13} /></span>{/if}
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

<style>
  /* Kept in an account's own playlists too (decision 0014), or pinned. */
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
