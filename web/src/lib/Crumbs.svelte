<script lang="ts">
  // A panel's title as a breadcrumb (0.9.3; the owner, 2026-10-25: Requests, Downloads, Insights, Health and Settings
  // get "pure breadcrumbs", "2 at most. So the current and one level above."): the level above, a tap back there, then
  // where one is. The page's own name above does what the panel's ✕ does; without a way back (`go`), it is only said.
  // No level above (`up` left out): only where one is, as beside the page whose name its list already shows (the
  // owner, 2026-10-25: Settings › Display beside Settings' list said Settings twice).
  // Where one is, cut short by …: a tap on it shows the whole line (the owner, 2026-10-25: "unellipse the whole
  // breadcrumb tree and align right so the own title is most definitely visible"), scrolled to its end; again, short.
  import { tick } from 'svelte';

  let { up = null, here }: { up?: { label: string; go?: () => void } | null; here: string } = $props();
  let line = $state<HTMLElement>();
  let hereEl = $state<HTMLElement>();
  let whole = $state(false);
  $effect(() => {
    void here;
    whole = false;
  });
  async function show() {
    if (whole) return void (whole = false);
    if (!hereEl || hereEl.scrollWidth <= hereEl.clientWidth) return;
    whole = true;
    await tick();
    line?.scrollTo({ left: line.scrollWidth });
  }
</script>

<h2 class="crumbs" class:whole bind:this={line}>
  {#if up}{#if up.go}<button class="up" onclick={up.go}>{up.label}</button>{:else}<span class="up">{up.label}</span>{/if}<span class="sep" aria-hidden="true">›</span>{/if}<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <span class="here" aria-current="page" bind:this={hereEl} onclick={show}>{here}</span>
</h2>

<style>
  /* As the panels' titles were: one line, the level above muted; each part shortened with … rather than the line wrap. */
  .crumbs {
    flex: 1;
    display: flex;
    align-items: baseline;
    min-width: 0;
    margin: 0;
    font-size: 20px;
    font-weight: 600;
    white-space: nowrap;
  }

  .up,
  .here {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Whole while there is room (the owner, 2026-10-25: "you have been limiting it too much"; a 45% cap of its own
     width cut it with room to spare); short of room, the level above gives way first, where one is last. */
  .up {
    flex: 0 1000 auto;
    color: var(--muted);
  }

  button.up {
    padding: 0;
    border: 0;
    background: none;
    font: inherit;
    cursor: pointer;
  }

  button.up:hover {
    color: var(--text);
  }

  .sep {
    flex: none;
    margin: 0 8px;
    color: var(--muted);
  }

  .here {
    flex: 0 1 auto;
  }

  /* The whole line: nothing cut, scrolled sideways (no bar), its end in view. */
  .crumbs.whole {
    overflow-x: auto;
    scrollbar-width: none;
  }

  .crumbs.whole::-webkit-scrollbar {
    display: none;
  }

  .crumbs.whole .up,
  .crumbs.whole .here {
    flex: none;
    overflow: visible;
  }
</style>
