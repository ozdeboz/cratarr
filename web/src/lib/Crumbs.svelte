<script lang="ts">
  // A panel's title as a breadcrumb (0.9.3; the owner, 2026-10-25: Requests, Downloads, Insights, Health and Settings
  // get "pure breadcrumbs"): `trail` the levels above, the top first, each a tap back there (without `go`, only said),
  // then where one is. Short (the owner: "2 at most. So the current and one level above"): the last level above and
  // where one is, both cut with … when short of room. Whole: every level, nothing cut, scrolled to its end (the owner,
  // 2026-10-25: "in scroll mode, the entire breadcrumb should be visible. In the other mode, only one level
  // difference"). A tap on where one is turns from one to the other while there is more to show (a level left out, or
  // something cut), for every panel on this device ("global on the device": prefs.crumbsWhole). No levels above: only
  // where one is, as beside the page whose name its list already shows (Settings › Display beside Settings' list said
  // Settings twice).
  import { tick } from 'svelte';
  import { prefs } from './prefs.svelte';

  type Level = { label: string; go?: () => void };
  let { trail = [], here }: { trail?: Level[]; here: string } = $props();
  let line = $state<HTMLElement>();
  const whole = $derived(prefs.crumbsWhole);
  const shown = $derived(whole ? trail : trail.slice(-1));
  // Whole: its end in view, whatever the panel or its title.
  $effect(() => {
    void here;
    if (whole && line) tick().then(() => line?.scrollTo({ left: line.scrollWidth }));
  });
  function show() {
    if (whole) return void (prefs.crumbsWhole = false);
    const cut = [...(line?.querySelectorAll<HTMLElement>('.up, .here') ?? [])].some((x) => x.scrollWidth > x.clientWidth);
    if (cut || trail.length > 1) prefs.crumbsWhole = true;
  }
</script>

<h2 class="crumbs" class:whole bind:this={line}>
  {#each shown as up, i (i)}{#if up.go}<button class="up" onclick={up.go}>{up.label}</button>{:else}<span class="up">{up.label}</span>{/if}<span class="sep" aria-hidden="true">›</span>{/each}<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <span class="here" aria-current="page" onclick={show}>{here}</span>
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
     width cut it with room to spare); short of room, both cut with …, each by its length (the owner, 2026-10-25: the
     level above giving way first left nothing of it beside a long name). */
  .up {
    flex: 0 1 auto;
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

  /* Scrolled, it runs on into the gap before the back button (App's .lead: 10 px), fading out across it there (the
     owner, 2026-10-25): from where it stopped hard to the button's edge. Not scrolled, nothing is in the gap. */
  .crumbs.whole {
    margin-left: -10px;
    padding-left: 10px;
    mask-image: linear-gradient(to right, transparent, #000 10px);
    -webkit-mask-image: linear-gradient(to right, transparent, #000 10px);
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
