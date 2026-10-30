<script lang="ts">
  // One entry in a menu sheet (decision 0021), as Finamp's: a 56 px row, a 24 px icon in the accent colour, the label
  // at 16 px. Destructive entries stay Cratarr's way: red (and last in the list). `checked` makes it one of Finamp's
  // toggle cards (the playlists for a track): filled with the accent, stronger when on, a mark at the right. `busy`:
  // its action is under way (an album going into a playlist track by track), the spinner in place of its label. `badge`:
  // a small icon after a toggle's label (a group's leader: a crown), `badgeLabel` saying what it means. `onmark`: the
  // toggle's mark (+ or the tick) is a button of its own, `markLabel` saying what it does; the label keeps `onclick`.
  // `plus: false`: no + while off (a choice that adds nothing), the tick only when on.
  // `switched`: an entry that turns something on or off (Sync to this device), its switch at the end. `sub`: a toggle's
  // second line, smaller, after `subIcon` (another player's track and whether it plays, in Play On; no cover).
  // `progress` (0 to 1): the row fills behind its label as its work goes (Download… to this device, lib/DownloadEntry).
  import Icon, { type IconName } from './Icon.svelte';
  import SwitchTrack from './SwitchTrack.svelte';

  let {
    icon = null,
    label,
    onclick,
    disabled = false,
    danger = false,
    checked = null,
    busy = false,
    badge = null,
    badgeLabel = '',
    onmark = null,
    markLabel = '',
    switched = null,
    plus = true,
    sub = null,
    subIcon = null,
    progress = null,
  }: {
    icon?: IconName | null;
    label: string;
    onclick: () => void;
    disabled?: boolean;
    danger?: boolean;
    checked?: boolean | null;
    busy?: boolean;
    badge?: IconName | null;
    badgeLabel?: string;
    onmark?: (() => void) | null;
    markLabel?: string;
    switched?: boolean | null;
    plus?: boolean;
    sub?: string | null;
    subIcon?: IconName | null;
    progress?: number | null;
  } = $props();
</script>

{#snippet name()}
  <span class:badged={badge !== null}
    >{#if badge}<em>{label}</em><b title={badgeLabel} aria-label={badgeLabel}><Icon name={badge} size={18} /></b>{:else}{label}{/if}</span
  >
{/snippet}
{#snippet text()}
  {#if sub}<span class="two">{@render name()}<small>{#if subIcon}<Icon name={subIcon} size={12} />{/if}<span>{sub}</span></small></span
    >{:else}{@render name()}{/if}
{/snippet}

{#if switched !== null}
  <button class="entry" role="menuitemcheckbox" aria-checked={switched} {disabled} {onclick}>
    {#if icon}<Icon name={icon} size={24} />{/if}<span>{label}</span><SwitchTrack on={switched} />
  </button>
{:else if checked === null}
  <button
    class="entry"
    class:danger
    class:filling={progress !== null}
    style:--fill={progress === null ? null : `${Math.min(1, Math.max(0, progress)) * 100}%`}
    role="menuitem"
    disabled={disabled || busy}
    aria-busy={busy || progress !== null}
    {onclick}
  >
    {#if icon}<Icon name={icon} size={24} />{/if}<span>{label}</span>
  </button>
{:else if onmark}
  <div class="toggle split" class:on={checked}>
    <button class="name" role="menuitemradio" aria-checked={checked} disabled={disabled || busy} {onclick}>
      {@render text()}
    </button><i aria-hidden="true"></i><button class="mark" aria-label={markLabel} title={markLabel} disabled={disabled || busy} onclick={onmark}
      ><Icon name={checked ? 'check' : 'plus'} size={22} /></button
    >
  </div>
{:else}
  <button class="toggle" class:on={checked} role="menuitemcheckbox" aria-checked={checked} disabled={disabled || busy} aria-busy={busy} {onclick}>
    {@render text()}{#if checked || plus}<i aria-hidden="true"></i><Icon name={checked ? 'check' : 'plus'} size={22} />{/if}
  </button>
{/if}

<style>
  .entry {
    display: flex;
    align-items: center;
    gap: 16px;
    width: 100%;
    min-height: 56px;
    padding: 0 24px 0 24px;
    background: none;
    border: 0;
    color: var(--text);
    font: inherit;
    font-size: 16px;
    text-align: left;
    cursor: pointer;
  }

  .entry :global(svg) {
    flex: none;
    color: var(--art-accent, var(--accent));
  }

  .entry:hover,
  .entry:focus-visible {
    background: color-mix(in srgb, var(--text) 7%, transparent);
  }

  .entry:disabled {
    color: color-mix(in srgb, var(--text) 38%, transparent);
    cursor: default;
  }

  .entry:disabled :global(svg) {
    opacity: 0.5;
  }

  /* Filled as far as its work has gone. */
  .entry.filling {
    background: linear-gradient(
      to right,
      color-mix(in srgb, var(--art-accent, var(--accent)) 22%, transparent) var(--fill),
      transparent var(--fill)
    );
    cursor: default;
  }

  .entry.danger,
  .entry.danger :global(svg) {
    color: var(--danger);
  }

  .toggle {
    box-sizing: border-box;
    display: flex;
    align-items: center;
    gap: 12px;
    width: calc(100% - 24px);
    min-height: 48px;
    margin: 4px 12px;
    padding: 0 14px;
    border: 0;
    border-radius: 12px;
    background: color-mix(in srgb, var(--art-accent, var(--accent)) 10%, transparent);
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .toggle.on {
    background: color-mix(in srgb, var(--art-accent, var(--accent)) 30%, transparent);
  }

  .toggle span {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Two buttons in one card: the label's, and the mark's (a thumb's width). */
  /* The divider and the mark where a single toggle has them (14 px in, 22 px icon, 12 px gap). */
  .toggle.split {
    gap: 0;
    padding: 0;
    cursor: default;
  }

  .toggle.split button {
    display: flex;
    align-items: center;
    align-self: stretch;
    background: none;
    border: 0;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .toggle.split .name {
    flex: 1;
    min-width: 0;
    padding: 0 12px 0 14px;
  }

  .toggle.split .mark {
    flex: none;
    justify-content: center;
    width: 48px;
  }

  /* A second line under the name: smaller, muted, its own ellipsis. */
  .toggle span.two {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .toggle span.two > span {
    flex: none;
  }

  .toggle span.two small {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    color: var(--muted);
    font-size: 13px;
  }

  .toggle span.two small :global(svg) {
    flex: none;
  }

  .toggle span.badged {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .toggle span.badged em {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    font-style: normal;
  }

  .toggle span.badged b {
    display: flex;
    flex: none;
  }

  .toggle i {
    align-self: stretch;
    width: 1.5px;
    margin: 10px 0;
    background: color-mix(in srgb, var(--text) 50%, transparent);
  }

  .toggle :global(svg) {
    flex: none;
    color: var(--art-accent, var(--accent));
  }

  .toggle:disabled {
    opacity: 0.5;
    cursor: default;
  }

  /* Under way: app.css's spinner alone in its place, at full strength (the icons keep their own colour, so they hide). */
  .toggle[aria-busy='true'] {
    opacity: 1;
  }

  [aria-busy='true'] :global(svg),
  [aria-busy='true'] i {
    visibility: hidden;
  }
</style>
