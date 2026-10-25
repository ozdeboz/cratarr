<script lang="ts">
  // What went wrong with something on a cover (the owner, 2026-10-26: a request not handed over, a play that failed;
  // its round mark a red "!", the message here instead of in the card's words): the message, Try again, Cancel.
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';

  let { title, text, retry = null, onclose }: { title: string; text: string; retry?: (() => void) | null; onclose: () => void } = $props();
</script>

<MenuSheet label={title} icon="bang" {title} {onclose}>
  <p class="text">{text}</p>
  {#if retry}
    <MenuEntry
      icon="reset"
      label="Try again"
      onclick={() => {
        retry?.();
        onclose();
      }}
    />
  {/if}
  <MenuEntry icon="close" label="Cancel" onclick={onclose} />
</MenuSheet>

<style>
  .text {
    margin: 0;
    padding: 4px 24px 12px;
    color: var(--danger-ink);
    overflow-wrap: anywhere;
  }
</style>
