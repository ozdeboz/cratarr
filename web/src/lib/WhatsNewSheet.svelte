<script lang="ts">
  // What's New (UC03; UC-Q2 (a): a sheet in Cratarr, the notes as the server parsed them): each version a heading, its parts'
  // headings and lines under it, newest first. Opened from About (lib/AboutSheet); App shows it (lib/whatsNew).
  import MenuSheet from './MenuSheet.svelte';
  import { whatsNew } from './whatsNew.svelte';
  import { noteTitle } from './whatsNewRule';

  let { onclose }: { onclose: () => void } = $props();
  const notes = $derived(whatsNew.value?.notes ?? []);
</script>

<MenuSheet label="What's new" title="What's new" icon="sparkles" {onclose}>
  <div class="notes">
    {#each notes as n (n.version)}
      <section>
        <h3>{noteTitle(n)}</h3>
        {#each n.parts as part, i (i)}
          {#if part.heading}<h4>{part.heading}</h4>{/if}
          {#if part.lines.length}<ul>{#each part.lines as line, j (j)}<li>{line}</li>{/each}</ul>{/if}
        {/each}
      </section>
    {/each}
  </div>
</MenuSheet>

<style>
  /* Padding, not margin: the sheet measures its content's height, which a child's margin would fall outside of (as About's). */
  .notes {
    display: grid;
    gap: 20px;
    padding: 4px 24px 20px;
    font-size: 15px;
  }

  h3 {
    margin: 0 0 6px;
    font-size: 15px;
    font-weight: 600;
  }

  h4 {
    margin: 10px 0 4px;
    color: var(--muted);
    font-size: 13px;
    font-weight: 600;
  }

  ul {
    display: grid;
    gap: 4px;
    margin: 0;
    padding-left: 18px;
  }

  li {
    overflow-wrap: anywhere;
  }
</style>
