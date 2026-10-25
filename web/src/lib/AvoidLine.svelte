<script lang="ts">
  // A ranked crate's "Avoiding …" line (the owner, 2026-10-23: playlist fit models, every ranked crate's): what it has
  // learned not to suggest, from what was turned down for it, and how many rules of one's own; a tap opens its Not for
  // sheet (lib/NotForSheet) to dismiss a trait, write a rule or put a track back. `round`: read again when it changes
  // (something turned down or put back).
  import { getCrateFit, traitLabel, type CrateFit } from './api';
  import Icon from './Icon.svelte';
  import NotForSheet from './NotForSheet.svelte';

  let { crate, name, round = 0, onchange = () => {} }: { crate: string; name: string; round?: number; onchange?: () => void } = $props();

  let fit = $state<CrateFit | null>(null);
  let open = $state(false);
  let saved = $state(0);
  /** The crate `fit` is of: another one's line goes at once, not shown while this one's comes; a read again keeps it
   *  (the owner, 2026-10-24). */
  let fitOf = '';
  $effect(() => {
    const c = crate;
    void [round, saved];
    if (c !== fitOf) {
      fitOf = c;
      fit = null;
    }
    getCrateFit(c).then(
      (f) => {
        if (c === crate) fit = f;
      },
      () => (fit = null),
    );
  });
  const avoiding = $derived((fit?.learned ?? []).filter((l) => !l.dismissed).map((l) => traitLabel(l.trait)));
  const rules = $derived(fit?.rules.length ?? 0);
</script>

{#if fit}
  <button class="avoid" onclick={() => (open = true)}>
    <Icon name="sliders" size={16} />
    <span>
      {#if avoiding.length}Avoiding {#each avoiding as a, i (a)}{#if i}{' · '}{/if}<b>{a}</b>{/each}{:else}Nothing avoided yet{/if}{#if rules}{' · '}{rules} {rules === 1 ? 'rule' : 'rules'}{/if}
    </span>
  </button>
{/if}
{#if open}
  <NotForSheet
    {crate}
    {name}
    onchange={() => {
      saved++;
      onchange();
    }}
    onclose={() => (open = false)}
  />
{/if}

<style>
  .avoid {
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: calc(100% - 32px);
    margin: 4px 16px 6px;
    padding: 8px 12px;
    border: 0;
    border-radius: 12px;
    background: color-mix(in srgb, var(--text) 6%, transparent);
    color: var(--muted);
    font: inherit;
    font-size: 0.88rem;
    text-align: start;
    cursor: pointer;
  }

  .avoid :global(svg) {
    flex: none;
    color: var(--accent);
  }

  .avoid span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  b {
    color: var(--text);
    font-weight: 500;
  }
</style>
