<script lang="ts">
  // The app's one ask at its start (lib/askAtStart), on Settings' own sheet for a question.
  import { backgroundAsk } from './askAtStart.svelte';
  import ConfirmSheet from './settings/ConfirmSheet.svelte';
</script>

{#if backgroundAsk.due}
  <ConfirmSheet
    title="Keep playing in the background?"
    text="Android may stop Cratarr otherwise."
    label="Allow"
    icon="play"
    danger={false}
    onconfirm={backgroundAsk.allow}
    onclose={backgroundAsk.asked}
  />
{/if}
