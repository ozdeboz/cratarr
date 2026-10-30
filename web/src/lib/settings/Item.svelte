<script lang="ts">
  // A row of the settings (the settings' North Star, the owner 2026-10-19), in a group (lib/settings/Group): its name,
  // at most one short line under it, and one thing at its end. What it does decides that end:
  //   - `toggle`: on or off, the whole row a switch;
  //   - `onclick`: it opens something (a thing's own page, a value's sheet): its value, if it has one, and ›;
  //   - `kind: 'add'` (accent, a plus in front) and `kind: 'danger'` (red): the row is the action itself;
  //   - children: a control of its own (a choice of a few, a button), which goes under the name where both do not fit.
  // `lead`: a dot in front (on, off, warn, bad) or a colour's swatch. `more`: a longer text behind an (i), beside the
  // row's control and never inside it (lib/MoreInfo). `progress` (0 to 1): work under way with a count, its line shown
  // as the one Meter (ui-conventions, Progress).
  import type { Snippet } from 'svelte';
  import Icon from '../Icon.svelte';
  import Meter from '../Meter.svelte';
  import MoreInfo from '../MoreInfo.svelte';
  import SwitchTrack from '../SwitchTrack.svelte';
  import { matchesSearch } from '../pageView.svelte';
  import { inGroupHit, settingsNarrowed } from './narrow';

  let {
    name,
    line = null,
    tone = null,
    wrap = false,
    more = null,
    experimental = false,
    lead = null,
    value = null,
    swatch = null,
    onclick = null,
    toggle = null,
    kind = null,
    disabled = false,
    busy = false,
    progress = null,
    children,
  }: {
    name: string;
    line?: string | null;
    /** The line's tone: a warning, or what is wrong. */
    tone?: 'warn' | 'bad' | null;
    /** The line may take more than one line (an address, a path). */
    wrap?: boolean;
    more?: string | null;
    /** Its (i) red, saying it is experimental. */
    experimental?: boolean;
    /** A dot ('on', 'off', 'warn', 'bad'), a colour, or '' for the room of one. */
    lead?: string | null;
    value?: string | null;
    /** A colour shown at the end, as a value. */
    swatch?: string | null;
    onclick?: (() => void) | null;
    toggle?: { checked: boolean; onchange: (on: boolean) => void } | null;
    kind?: 'add' | 'danger' | null;
    disabled?: boolean;
    busy?: boolean;
    progress?: number | null;
    children?: Snippet;
  } = $props();

  const act = $derived(toggle ? () => toggle.onchange(!toggle.checked) : onclick);
  const DOTS = ['on', 'off', 'warn', 'bad'];
  const opens = $derived(!!onclick && !kind && !toggle);
  // Left out by the panel's own search on a phone (lib/settings/narrow), unless its group's title has the words: read
  // in its name and all it shows (its line, its value, its choices' labels: "dark" finds Theme).
  const narrowed = settingsNarrowed();
  const groupHas = inGroupHit();
  let el = $state<HTMLElement>();
  /** Its texts apart (its choices' labels side by side would read as one word). */
  function shown(node: HTMLElement | undefined) {
    if (!node) return '';
    const parts: string[] = [];
    const walk = document.createTreeWalker(node, NodeFilter.SHOW_TEXT);
    while (walk.nextNode()) parts.push(walk.currentNode.textContent ?? '');
    return parts.join(' ');
  }
  const left = $derived(narrowed() && !groupHas() && !matchesSearch(name, shown(el)));
</script>

{#snippet body()}
  {#if kind === 'add'}<span class="lead plus"><Icon name="plus" size={18} /></span>{:else if lead !== null}
    <!-- '' keeps the lead's room, so its name lines up with the rows beside it that have one. -->
    <span class="lead">{#if DOTS.includes(lead)}<span class="dot {lead}"></span>{:else if lead}<span class="colour" style:background={lead}></span>{/if}</span>
  {/if}
  <span class="text">
    <span class="name">{name}</span>
    {#if progress !== null && line}<Meter label={line} value={progress} />{:else if line}<small class:wrap class:warn={tone === 'warn'} class:bad={tone === 'bad'}>{line}</small>{/if}
  </span>
{/snippet}

<div class="item" bind:this={el} class:left class:act={!!act} class:add={kind === 'add'} class:danger={kind === 'danger'} class:off={disabled}>
  {#if act}
    <button
      class="main"
      type="button"
      role={toggle ? 'switch' : undefined}
      aria-checked={toggle ? toggle.checked : undefined}
      {disabled}
      aria-busy={busy || undefined}
      onclick={act}>{@render body()}</button
    >
  {:else}
    <div class="main">{@render body()}</div>
  {/if}
  {#if more}<span class="more"><MoreInfo label={name} text={more} {experimental} /></span>{/if}
  {#if act && (toggle || opens)}
    <!-- The row's end is the row too (a second way to the same act, kept out of the tab order). -->
    <button class="end" type="button" tabindex="-1" aria-hidden="true" {disabled} onclick={act}>
      {#if toggle}
        <SwitchTrack on={toggle.checked} />
      {:else}
        {#if swatch}<span class="colour" style:background={swatch}></span>{/if}
        {#if value}<span class="value">{value}</span>{/if}
        <Icon name="chevronRight" size={20} />
      {/if}
    </button>
  {:else if children || value || swatch}
    <!-- A value only shown (the account's name), or a control of its own. -->
    <span class="end own">
      {#if swatch}<span class="colour" style:background={swatch}></span>{/if}
      {#if value}<span class="value shown">{value}</span>{/if}
      {@render children?.()}
    </span>
  {/if}
</div>

<style>
  .item.left {
    display: none;
  }

  .item {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    min-height: 56px;
    padding: 8px 14px 8px 16px;
  }

  /* A row that acts: its buttons fill it, so all of it but the (i) answers a tap. */
  .item.act {
    flex-wrap: nowrap;
    gap: 0;
    padding: 0;
  }

  .item.act:hover:not(.off) {
    background: color-mix(in srgb, var(--text) 5%, transparent);
  }

  .item.off > :not(.more) {
    opacity: 0.5;
  }

  /* A control of its own (a choice) takes no tap while the row does not apply. */
  .item.off .own {
    pointer-events: none;
  }

  /* The name's room: little reserved (6em, it was 10), so a short choice fits beside it on a phone instead of a line
     of its own (the owner, 2026-10-25). */
  .main {
    display: flex;
    flex: 1 1 6em;
    align-items: center;
    gap: 14px;
    min-width: 0;
  }

  button.main {
    align-self: stretch;
    min-height: 56px;
    padding: 8px 8px 8px 16px;
    border: 0;
    border-radius: 0;
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  button.main:focus-visible,
  button.end:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .item.act .more {
    padding-right: 4px;
  }

  .more {
    display: flex;
    flex: none;
  }

  .end {
    display: flex;
    flex: none;
    align-items: center;
    gap: 8px;
    color: var(--muted);
  }

  button.end {
    align-self: stretch;
    padding: 0 14px 0 4px;
    border: 0;
    border-radius: 0;
    background: none;
    font: inherit;
    cursor: pointer;
  }

  button:disabled {
    cursor: default;
  }

  /* A control of its own: its width, never wider than the row (then under the name). */
  .end.own {
    flex: 0 1 auto;
    flex-wrap: wrap;
    justify-content: flex-end;
    max-width: 100%;
    min-width: 0;
    color: var(--text);
  }

  .value.shown {
    color: var(--muted);
  }

  .value {
    max-width: 16em;
    overflow: hidden;
    font-size: 15px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .lead {
    display: grid;
    flex: none;
    place-items: center;
    width: 10px;
  }

  .lead.plus {
    width: 18px;
    color: var(--accent-ink);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
  }

  .dot.off {
    background: var(--muted);
  }

  .dot.warn {
    background: var(--warn);
  }

  .dot.bad {
    background: var(--danger);
  }

  .colour {
    flex: none;
    width: 18px;
    height: 18px;
    border-radius: 50%;
  }

  .lead .colour {
    width: 10px;
    height: 10px;
  }

  .text {
    display: grid;
    flex: 1;
    gap: 2px;
    min-width: 0;
  }

  .name {
    overflow-wrap: anywhere;
    font-size: 15px;
  }

  .add .name {
    color: var(--accent-ink);
  }

  .danger .name {
    color: var(--danger-ink);
  }

  small {
    overflow: hidden;
    color: var(--muted);
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  small.wrap {
    overflow-wrap: anywhere;
    white-space: normal;
  }

  small.warn {
    color: var(--warn-ink);
  }

  small.bad {
    color: var(--danger-ink);
  }

  /* A row's own buttons: small and flat, the main one in the accent. */
  .own :global(button:where(:not(.info, .switch, [aria-pressed]))) {
    min-height: 36px;
    padding: 0 14px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg);
    color: var(--text);
    font: inherit;
    font-size: 14px;
    cursor: pointer;
  }

  .own :global(button.primary) {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  .own :global(button:disabled) {
    opacity: 0.5;
    cursor: default;
  }
</style>
