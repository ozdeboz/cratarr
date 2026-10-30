<script lang="ts">
  // A cover whose album is being opened, when that takes long (the owner, 2026-10-27): after OPEN_WAIT_MS it dims, with
  // everything on it, and the ring turns over it, as the player's cover waits for a new one (lib/NowPlaying). Only the
  // cover opened, never a cover's picture loading (the owner: not many signs at once). Over the cover, inside what
  // holds it (positioned); over its buttons too.
  import { opening } from './panel.svelte';
  import ProgressRing from './ProgressRing.svelte';

  let { subject, size = 40 }: { subject: string | null; size?: number } = $props();

  const OPEN_WAIT_MS = 300;
  const asked = $derived(subject !== null && opening(subject));
  let slow = $state(false);
  $effect(() => {
    if (!asked) return void (slow = false);
    const t = setTimeout(() => (slow = true), OPEN_WAIT_MS);
    return () => clearTimeout(t);
  });
</script>

<span class="cover-wait" class:on={slow} aria-hidden="true">{#if slow}<ProgressRing progress={null} {size} spin />{/if}</span>

<style>
  .cover-wait {
    position: absolute;
    inset: 0;
    z-index: 2;
    border-radius: inherit;
    background: rgb(0 0 0 / 0.7);
    opacity: 0;
    pointer-events: none;
    transition: opacity 400ms ease-out;
  }

  .cover-wait.on {
    opacity: 1;
    pointer-events: auto;
  }
</style>
