<script lang="ts" generics="T extends string | boolean">
  // A choice of one (decided 2026-10-16: shared pieces; the owner, 2026-11-02: one row, its end says what kind): rows on
  // the sheet's card, an icon at the start where a choice has one, its words and a line saying what it does, and the
  // round mark at the end (lib/ChoiceMark), the chosen one's filled. View's order, More music's Radio and Autoplay, the
  // radio sheet, every ChoiceSheet.
  import ChoiceMark from './ChoiceMark.svelte';
  import Icon, { type IconName } from './Icon.svelte';

  let {
    label,
    options,
    value,
    onchange,
  }: {
    /** The group's accessible name. */
    label: string;
    options: { value: T; label: string; line?: string | null; icon?: IconName }[];
    value: T;
    onchange: (value: T) => void;
  } = $props();
</script>

<div class="choices" role="radiogroup" aria-label={label}>
  {#each options as o (o.value)}
    {@const on = o.value === value}
    <button type="button" role="radio" aria-checked={on} onclick={() => onchange(o.value)}
      >{#if o.icon}<Icon name={o.icon} size={22} />{/if}<span class="words"><span>{o.label}</span>{#if o.line}<small>{o.line}</small>{/if}</span><ChoiceMark {on} /></button
    >
  {/each}
</div>

<style>
  /* The sheet's one card, as Settings' groups. */
  .choices {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    border-radius: 14px;
    background: var(--card, color-mix(in srgb, var(--text) 6%, transparent));
  }

  /* In a titled card (lib/Card) its rows are the card's own. */
  :global(.card) .choices {
    margin: 0 -12px 0 -16px;
    border-radius: 0;
    background: none;
  }

  :global(.card) button {
    padding-inline: 16px 12px;
  }

  button {
    display: flex;
    align-items: center;
    gap: 14px;
    min-height: 52px;
    padding: 8px 14px;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    font-size: 15px;
    text-align: left;
    cursor: pointer;
  }

  button + button {
    border-top: 1px solid var(--border);
  }

  button > :global(svg) {
    flex: none;
    color: var(--muted);
  }

  .words {
    display: grid;
    flex: 1;
    min-width: 0;
  }

  small {
    color: var(--muted);
    font-size: 13px;
  }
</style>
