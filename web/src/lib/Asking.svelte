<script lang="ts">
  // A list's place while its answer is on its way (lib/loader: the one rule): the mark in motion (lib/MarkLoader; the
  // owner, 2026-10-31: a spinner while the page swaps its content for the search's). `now`: what the list showed was
  // kept until the loader was due (lib/keepWhile), so it shows at once; else nothing was there to keep, and it comes
  // LOADER_AFTER_MS after the wait began, as it would have. Its room is kept from the start, so its coming moves nothing.
  // In a panel or a sheet it is the small one (lib/loader), as every loader there. Every wait but a page's whole content (lib/PageLoading)
  // is this one, so each says what it waits for the same way (lib/waiting, 0.17). `room`: its own (a list's place), the one the
  // place around it gives (where more of a list is coming, the panel's swap), or a line of text (a count still counting).
  import MarkLoader from './MarkLoader.svelte';
  import PageLoading from './PageLoading.svelte';
  import { waitingFor } from './waiting.svelte';
  import { LOADER_AFTER_MS, LOADER_LARGE, loaderSizeIn } from './loader';

  // `page`: a list that is a page's whole content (Playlists, Crates, the Library, everywhere's results): the mark in the middle of
  // the room the page has left, as a page's loader (lib/PageLoading), not at the list's top (the owner, 2026-11-19: "On page lists,
  // the loading spinner should be vertically centered").
  let { size = 48, now = false, room = 'own' }: { size?: number; now?: boolean; room?: 'own' | 'given' | 'inline' | 'page' } = $props();

  let el = $state<HTMLElement>();
  /** The most a loader is where this is. */
  let most = $state(LOADER_LARGE);
  let due = $state(false);
  $effect(() => {
    if (el) most = loaderSizeIn(el);
    const t = setTimeout(() => (due = true), LOADER_AFTER_MS);
    return () => clearTimeout(t);
  });
</script>

{#if room === 'page'}<PageLoading held={false} shown={now || due} />{:else}<svelte:element this={room === 'inline' ? 'span' : 'div'} class={room === 'inline' ? 'inline' : 'asking'} class:given={room === 'given'} aria-busy="true" bind:this={el}>
  {#if now || due}<MarkLoader size={Math.min(size, most)} />{#if waitingFor()}{#if room === 'inline'}<span>{waitingFor()}</span>{:else}<p class="for">{waitingFor()}</p>{/if}{/if}{/if}
</svelte:element>{/if}

<style>
  .asking {
    display: grid;
    place-items: center;
    min-height: 96px;
    padding-top: 24px;
  }

  .asking.given {
    min-height: 0;
    padding-top: 0;
  }

  .inline {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }

  /* What it waits for (lib/waiting), as a page's loader says it under its mark (lib/PageLoading). */
  .for {
    margin: 12px 0 0;
    color: var(--muted);
    text-align: center;
  }
</style>
