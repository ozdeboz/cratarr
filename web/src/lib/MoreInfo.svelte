<script lang="ts">
  // A setting's longer explanation behind an (i), as Finamp's settings keep one short line and open the rest on demand
  // (SubtitleWithMoreInfoDialog; the owner, 2026-10-17: "we learn from the way Finamp presents settings and deeper
  // settings"). The (i) opens a sheet with the setting's name and the text; paragraphs are split on empty lines.
  import Icon from './Icon.svelte';
  import MenuSheet from './MenuSheet.svelte';

  /** `experimental`: a red (i), its sheet saying so first (the owner, 2026-10-26: experimental features marked). */
  let { label, text, experimental = false }: { label: string; text: string; experimental?: boolean } = $props();
  let open = $state(false);
  const EXPERIMENTAL = 'Experimental: it may not behave as described yet.';
  const paragraphs = $derived(text.split(/\n\s*\n/).map((p) => p.trim()).filter(Boolean));
</script>

<button class="info" class:experimental type="button" aria-label={`About ${label}`} title={`About ${label}`} onclick={() => (open = true)}><Icon name="info" size={18} /></button>
{#if open}
  <MenuSheet label={label} icon="info" title={label} onclose={() => (open = false)}>
    <div class="text">
      {#if experimental}<p class="experimental">{EXPERIMENTAL}</p>{/if}
      {#each paragraphs as p (p)}<p>{p}</p>{/each}
    </div>
  </MenuSheet>
{/if}

<style>
  .info {
    display: inline-grid;
    place-items: center;
    flex: none;
    width: 32px;
    height: 32px;
    margin: -6px 0;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--muted);
    vertical-align: middle;
    cursor: pointer;
  }

  .info:hover,
  .info:focus-visible {
    background: color-mix(in srgb, var(--text) 8%, transparent);
    color: var(--text);
  }

  .info.experimental,
  .text .experimental {
    color: var(--danger-ink);
  }

  .info.experimental:hover,
  .info.experimental:focus-visible {
    color: var(--danger);
  }

  /* Padding, not margin: the sheet measures its content's height, which a child's margin would fall outside of. */
  .text {
    padding: 4px 24px 20px;
    font-size: 15px;
    line-height: 1.5;
  }

  .text p {
    margin: 0 0 10px;
  }

  .text p:last-child {
    margin-bottom: 0;
  }
</style>
