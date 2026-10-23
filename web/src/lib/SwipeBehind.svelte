<script lang="ts">
  // What a row's swipe does (lib/swipe.ts), in the strip the row uncovers as it moves, growing with the move as Finamp's
  // and Android's do: a swipe right's on the row's left (Play next, in the accent), a swipe left's on its right (Remove,
  // in red). Put beside the swiped row, in its list item (which is positioned); a way without an action shows nothing.
  import Icon, { type IconName } from './Icon.svelte';

  type Action = { icon: IconName; label: string; danger?: boolean };
  let { left = null, right = null }: { left?: Action | null; right?: Action | null } = $props();
</script>

{#if right}<span class="behind start" class:danger={right.danger} aria-hidden="true"><span class="in"><Icon name={right.icon} size={32} /><span>{right.label}</span></span></span>{/if}
{#if left}<span class="behind end" class:danger={left.danger} aria-hidden="true"><span class="in"><span>{left.label}</span><Icon name={left.icon} size={32} /></span></span>{/if}

<style>
  .behind {
    position: absolute;
    top: var(--swipe-top, 0px);
    height: var(--swipe-height, 0px);
    display: flex;
    align-items: center;
    overflow: hidden;
    border-radius: 8px;
    background: color-mix(in srgb, currentColor 14%, transparent);
    color: var(--art-accent, var(--accent));
    font-size: 14px;
    font-weight: 500;
    white-space: nowrap;
    pointer-events: none;
    /* Springing back (or sliding out) with the row; while the finger moves it, at once. */
    transition:
      left 200ms linear,
      width 200ms linear;
  }

  :global([data-swiping]) > .behind {
    transition: none;
  }

  .danger {
    color: var(--danger-ink);
  }

  /* Held at the row's edge it was uncovered from; the strip clips it until there is room. */
  .in {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 16px;
  }

  .start {
    left: var(--swipe-left, 0px);
    width: max(0px, var(--swipe-x, 0px));
    justify-content: flex-start;
  }

  .end {
    left: calc(var(--swipe-left, 0px) + var(--swipe-width, 0px) + min(0px, var(--swipe-x, 0px)));
    width: max(0px, -1 * var(--swipe-x, 0px));
    justify-content: flex-end;
  }
</style>
