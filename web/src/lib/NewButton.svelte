<script lang="ts">
  // A page's "new" button (a playlist, a crate) in the page's top right corner: a + alone, and its label beside it once
  // the page is wide enough (a container query on the page, not the window: beside the player it can be narrow).
  import Icon from './Icon.svelte';

  let { label, onclick }: { label: string; onclick: (e: MouseEvent) => void } = $props();
</script>

<button class="corner" aria-label={label} {onclick}><Icon name="plus" size={22} /><span class="label">{label}</span></button>

<style>
  .corner {
    position: absolute;
    top: 14px;
    right: clamp(12px, 2vw, 32px);
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-width: 44px;
    min-height: 44px;
    padding: 0;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  .label {
    display: none;
  }

  @container (min-width: 560px) {
    .corner {
      padding: 0 14px 0 10px;
    }

    .label {
      display: inline;
    }
  }
</style>
