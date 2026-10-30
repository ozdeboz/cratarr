<script lang="ts">
  // A genre fix's genres to pick (0.9.8; the owner, 2026-10-27: "the option to use multiple found genres! Picking is
  // best in this screen"): the genre as it is and as it would be, then every genre and style found (Discogs with the
  // rest), strongest first, as a filter's chips (lib/ConditionCard); the strongest picked to begin with. Health's item
  // (lib/HealthFixes) and Incoming's intake (lib/IntakeCheck).
  import type { Fix } from './api';
  import Icon from './Icon.svelte';

  let { fix, picked = $bindable() }: { fix: Fix; picked: string[] } = $props();

  function pick(word: string) {
    picked = picked.includes(word) ? picked.filter((w) => w !== word) : [...picked, word];
  }
</script>

<span class="change"
  ><span class="now">{fix.from || '–'}</span><Icon name="arrowRight" size={14} /><b>{picked.join('; ') || '–'}</b></span
>
<span class="chips">
  {#each fix.candidates ?? [] as c (c.word)}
    <button class="chip" aria-pressed={picked.includes(c.word)} title="{c.source}{c.kind === 'style' ? ' · style' : ''}" onclick={() => pick(c.word)}
      >{c.word}</button
    >
  {/each}
</span>

<style>
  .change {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    overflow-wrap: anywhere;
  }

  .now {
    color: var(--muted);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 4px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    min-height: 32px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg);
    color: inherit;
    font: inherit;
    font-size: 14px;
    cursor: pointer;
  }

  .chip[aria-pressed='true'] {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 22%, var(--bg));
  }
</style>
