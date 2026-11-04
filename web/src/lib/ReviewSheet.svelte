<script lang="ts">
  // The review sheet (decision 0092's one look; the owner, 2026-11-03: "the lowest level should be a review sheet, not
  // a panel of its own"): a plan to review over where it was opened from (Health's finding, a record, Before Keep), and
  // once written, its run in the same sheet; closed, that place shows as it was. One for the whole page (App).
  import FileChanges from './FileChanges.svelte';
  import { review } from './fileRuns.svelte';
  import MenuSheet from './MenuSheet.svelte';

  const title = $derived(
    review.thing?.startsWith('run:')
      ? 'Written to files'
      : review.thing?.startsWith('intake:')
        ? 'Before Keep'
        : review.thing?.startsWith('renumber:')
          ? 'Renumber'
          : 'Review the fixes',
  );
</script>

{#if review.thing}
  <MenuSheet label={title} {title} onclose={() => (review.thing = null)}>
    <div class="review"><FileChanges thing={review.thing} /></div>
  </MenuSheet>
{/if}

<style>
  /* As in Health's panel: its records and words in from the sheet's edges. */
  .review {
    padding: 0 clamp(12px, 2vw, 24px) 8px;
  }
</style>
