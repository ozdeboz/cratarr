<script lang="ts">
  // Fetching the library whole on mobile data is asked first, once (lib/deviceSync), on Settings' own sheet for a question.
  import { answerMobile, indexSync } from './deviceSync.svelte';
  import { prefs } from './prefs.svelte';
  import ConfirmSheet from './settings/ConfirmSheet.svelte';
</script>

{#if indexSync.shows === 'asks'}
  <ConfirmSheet
    title="Fetch the library over mobile data?"
    text="So it works with Cratarr away."
    label="Fetch"
    icon="download"
    danger={false}
    onconfirm={() => void answerMobile(true)}
    onclose={() => void (prefs.indexMobile === 'unasked' && answerMobile(false))}
  />
{/if}
