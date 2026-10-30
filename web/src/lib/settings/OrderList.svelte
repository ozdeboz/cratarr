<script lang="ts" generics="T extends string">
  // Things switched on in the user's order (the owner: the player's row's facts, 2026-10-19; a list's profile rows,
  // 2026-10-23): those on first, in their order, each with a handle to drag (as My order in the output step), then the
  // rest in the order offered, each with its switch. With `fold`, a fold line among those on, dragged as they are: what
  // is above it stays when the thing is folded (a list's profile, the owner 2026-10-23), what is below folds away.
  import Icon from '../Icon.svelte';
  import { Reorder } from '../reorder.svelte';
  import SwitchTrack from '../SwitchTrack.svelte';

  let {
    items,
    on,
    label: name,
    onchange,
    fold = null,
    lead = null,
    unavailable = null,
  }: {
    /** All of them, in the order offered. */
    items: readonly { id: T; label: string }[];
    /** Those on, in their order. */
    on: readonly T[];
    /** The list's accessible name. */
    label: string;
    onchange: (on: T[]) => void;
    /** The fold line: how many of those on are above it. */
    fold?: { at: number; onchange: (at: number) => void } | null;
    /** A first place only some may take (Home's top: the flow; the owner, 2026-10-24): marked by a line under it; each
     *  that may take it, on or off, has the stars beside its name (`label` says what they mean; no word on the first,
     *  the owner the same day); one that may not is never dragged above the line, and the first stays unless the next
     *  may take it. */
    lead?: { may: (id: T) => boolean; label: string } | null;
    /** Those not to be had here (Home's in local mode, the owner 2026-10-24): greyed, `note` under their name, their
     *  switch showing the choice kept for them, neither switched nor dragged. */
    unavailable?: { has: (id: T) => boolean; note: string } | null;
  } = $props();
  const na = (id: T) => !!unavailable?.has(id);

  /** Those on, the fold line among them (null). */
  const entries = $derived<(T | null)[]>(fold ? [...on.slice(0, fold.at), null, ...on.slice(fold.at)] : [...on]);

  const label = (id: T) => items.find((f) => f.id === id)?.label ?? id;
  const off = $derived(items.filter((f) => !on.includes(f.id)).map((f) => f.id));

  function flip(id: T) {
    navigator.vibrate?.(8);
    // One switched off above the fold line takes the line up with it; one switched on comes in last, below it.
    if (fold && on.indexOf(id) >= 0 && on.indexOf(id) < fold.at) fold.onchange(fold.at - 1);
    onchange(on.includes(id) ? on.filter((x) => x !== id) : [...on, id]);
  }
  let list = $state<HTMLElement>();
  const reorder = new Reorder(
    () => list,
    () => entries.length,
    (from, to) => {
      const v = [...entries];
      const [x] = v.splice(from, 1);
      v.splice(to, 0, x);
      onchange(v.filter((e): e is T => e !== null));
      if (fold) fold.onchange(v.indexOf(null));
    },
    (from) => {
      const last = entries.length - 1;
      if (!lead) return [0, last];
      const id = entries[from];
      if (from === 0) return entries[1] !== undefined && entries[1] !== null && lead.may(entries[1]) ? [0, last] : [0, 0];
      return id !== null && lead.may(id) ? [0, last] : [1, last];
    },
  );
</script>

{#snippet shownName(id: T)}
  <span
    >{label(id)}{#if lead?.may(id)}<span class="may" title={lead.label} aria-label={lead.label}><Icon name="sparkles" size={16} /></span>{/if}{#if na(id)}<span
        class="note">{unavailable?.note}</span
      >{/if}</span
  >
{/snippet}

<ol class="facts" class:dragging={!!reorder.drag} bind:this={list} aria-label={name}>
  {#each entries as id, i (id ?? '|')}
    <li class:lifted={reorder.drag?.from === i} class:line={id === null} class:lead={!!lead && i === 0} style:transform={reorder.drag ? `translateY(${reorder.offset(i)}px)` : null}>
      {#if id === null}
        <!-- The fold line, as the profile draws it (lib/ListProfile): above it stays, below it folds. -->
        <span class="row fold"><span class="pill"><Icon name="chevronUp" size={14} />Fold</span></span>
      {:else}
        <button class="row" class:na={na(id)} role="switch" aria-checked="true" disabled={na(id)} onclick={() => flip(id)}
          >{@render shownName(id)}<SwitchTrack on={true} /></button
        >
      {/if}
      {#if id !== null && na(id)}<span class="grip-room" aria-hidden="true"></span>{:else}<button
        class="grip"
        aria-label={id === null ? 'Drag the fold line' : `Drag ${label(id)} to reorder`}
        onpointerdown={(e) => reorder.down(e, i)}
        onpointermove={reorder.move}
        onpointerup={reorder.end}
        onpointercancel={reorder.end}><Icon name="order" size={22} /></button
      >{/if}
    </li>
  {/each}
  {#each off as id (id)}
    <li>
      <button class="row" class:na={na(id)} role="switch" aria-checked="false" disabled={na(id)} onclick={() => flip(id)}>{@render shownName(id)}<SwitchTrack on={false} /></button>
      <span class="grip-room" aria-hidden="true"></span>
    </li>
  {/each}
</ol>

<style>
  .facts {
    position: relative;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .facts li {
    position: relative;
    display: flex;
    align-items: center;
    background: var(--surface);
  }

  /* The first place, for those that may take it: a line under it, its name beside the first's. */
  .facts li.lead {
    border-bottom: 2px solid var(--accent);
  }

  /* Can take the first place: the stars, small, after the name. */
  .may {
    display: inline-flex;
    margin-left: 8px;
    color: var(--accent);
    vertical-align: -2px;
  }

  .facts.dragging li {
    transition: transform 200ms ease;
  }

  .facts.dragging li.lifted {
    z-index: 1;
    transition: none;
  }

  .row {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-height: 48px;
    padding: 0 16px 0 0;
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  /* Not to be had here: greyed, its note under its name. */
  .row.na {
    opacity: 0.5;
    cursor: default;
  }

  .note {
    display: block;
    color: var(--muted);
    font-size: 0.8rem;
  }

  /* The fold line: a rule with its pill on it. */
  .row.fold {
    position: relative;
    justify-content: center;
    cursor: default;
  }

  .row.fold::before {
    content: '';
    position: absolute;
    top: 50%;
    right: 16px;
    left: 0;
    border-top: 1px dashed var(--border);
  }

  .pill {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 12px;
    border: 1px solid var(--border);
    border-radius: 99px;
    background: var(--surface);
    color: var(--muted);
    font-size: 13px;
  }

  /* The handle in front, the switch at the end (the owner, 2026-10-25: Home's sections, "their order handlebars on
     the left and not on the right"); a row switched off keeps the handle's room, so the names line up. */
  .grip,
  .grip-room {
    order: -1;
    flex: none;
    width: 44px;
    height: 48px;
  }

  .grip {
    display: grid;
    place-items: center;
    padding: 0;
    border: 0;
    background: none;
    color: var(--muted);
    touch-action: none;
    cursor: grab;
  }
</style>
