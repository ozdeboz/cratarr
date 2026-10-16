<script lang="ts" generics="T extends string | boolean">
  // A choice of one as rows with a round mark (decided 2026-10-16: shared pieces): View's order of a track list, More
  // music's Radio and Autoplay. The chosen one's mark is filled in the accent.
  import Icon from './Icon.svelte';

  let {
    label,
    options,
    value,
    onchange,
  }: {
    /** The group's accessible name. */
    label: string;
    options: { value: T; label: string }[];
    value: T;
    onchange: (value: T) => void;
  } = $props();
</script>

<div class="choices" role="radiogroup" aria-label={label}>
  {#each options as o (o.value)}
    {@const on = o.value === value}
    <button role="radio" aria-checked={on} class:on onclick={() => onchange(o.value)}><Icon name={on ? 'circleCheck' : 'circle'} size={22} />{o.label}</button>
  {/each}
</div>

<style>
  .choices {
    display: flex;
    flex-direction: column;
  }

  button {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 44px;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    font-size: 15px;
    text-align: left;
    cursor: pointer;
  }

  button.on :global(svg) {
    color: var(--art-accent, var(--accent));
  }
</style>
