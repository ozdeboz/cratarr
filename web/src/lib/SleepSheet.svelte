<script lang="ts">
  // The sleep timer's choices (the player's moon, lib/sleep.svelte.ts): a time from now, the end of the playing track,
  // or off while one runs. Its card shows what runs now.
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { setSleep, sleep, sleepLeft } from './sleep.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const CHOICES: { label: string; minutes: number }[] = [
    { label: '15 minutes', minutes: 15 },
    { label: '30 minutes', minutes: 30 },
    { label: '45 minutes', minutes: 45 },
    { label: '1 hour', minutes: 60 },
    { label: '2 hours', minutes: 120 },
  ];

  const running = $derived(sleep());
  const left = $derived(sleepLeft());

  function pick(to: Parameters<typeof setSleep>[0]) {
    setSleep(to);
    onclose();
  }
</script>

<MenuSheet label="Sleep timer" icon="moon" title="Sleep timer" chips={left ? [left] : []} {onclose}>
  <div role="menu">
    {#each CHOICES as c (c.minutes)}
      <MenuEntry label={c.label} onclick={() => pick({ minutes: c.minutes })} />
    {/each}
    <MenuEntry label="End of track" onclick={() => pick('endOfTrack')} />
    {#if running}
      <MenuEntry label="Off" onclick={() => pick(null)} />
    {/if}
  </div>
</MenuSheet>
