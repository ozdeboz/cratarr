<script lang="ts">
  // A panel's title as a breadcrumb (0.9.3; the owner, 2026-10-25: Requests, Downloads, Insights, Health and Settings
  // get "pure breadcrumbs"): `trail` the levels above, the top first, each a tap back there (without `go`, only said),
  // then where one is. Short (the owner: "2 at most. So the current and one level above"): the last level above and
  // where one is, both cut with … when short of room. Whole: every level, nothing cut, scrolled to its end (the owner,
  // 2026-10-25: "in scroll mode, the entire breadcrumb should be visible. In the other mode, only one level
  // difference"). A long-press, and only that (the owner, 2026-11-02), turns from one to the other while there is more
  // to show (a level left out, or something cut), for every panel on this device ("global on the device": prefs.crumbsWhole). No levels above: only
  // where one is, as beside the page whose name its list already shows (Settings › Display beside Settings' list said
  // Settings twice).
  // One rule for every panel (the owner, 2026-11-01): the levels are those Back walks through, each a tap back there;
  // only places are levels. Where one is can be a picker of its siblings (Settings' other parts, Health's other
  // problems: a move sideways instead of up and down again, as the Library's "Records ▾"). A long-press anywhere on it
  // switches short and whole (the owner, 2026-11-02: a tap is the pickers' now; the … before the trail that did it read
  // as a level of its own), and is never also their tap.
  // A level above can be its group (the owner, 2026-11-02: Settings › Device › Appearance, the parts' picker was "way too
  // tall"): a picker of the groups, landing on the part last seen in one (lib/groupCrumb). Not a place: Back walks past
  // it, and it is no tap back there.
  import { tick } from 'svelte';
  import { longpress } from './longpress';
  import { prefs } from './prefs.svelte';
  import GlassPicker, { type PickerOption } from './GlassPicker.svelte';

  export type Siblings = { menuLabel: string; options: readonly PickerOption[]; chosen: string | null; onchoose: (id: string) => void };
  /** A level above: a tap back there (without `go`, only said), or its group's picker (`pick`). */
  export type Level = { label: string; go?: () => void; pick?: Siblings };
  let { trail = [], here, siblings = null }: { trail?: Level[]; here: string; siblings?: Siblings | null } = $props();
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
    const cut = [...(line?.querySelectorAll<HTMLElement>('.up, .here, .label') ?? [])].some((x) => x.scrollWidth > x.clientWidth);
    if (cut || trail.length > 1) prefs.crumbsWhole = true;
  }
</script>

<h2 class="crumbs" class:whole bind:this={line} use:longpress={show}>
  {#each shown as up, i (i)}{#if up.pick}<span class="up pick"><GlassPicker shown={up.label} menuLabel={up.pick.menuLabel} options={up.pick.options} chosen={up.pick.chosen} onchoose={up.pick.onchoose} /></span>{:else if up.go}<button class="up" onclick={up.go}>{up.label}</button>{:else}<span class="up">{up.label}</span>{/if}<span class="sep" aria-hidden="true">›</span>{/each}{#if siblings}<GlassPicker shown={here} menuLabel={siblings.menuLabel} options={siblings.options} chosen={siblings.chosen} onchoose={siblings.onchoose} />{:else}<span class="here" aria-current="page">{here}</span>{/if}
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

  /* A group's picker as a level above: muted as the others, as large as the picker of where one is. */
  .pick {
    display: flex;
  }

  .pick :global(button.shown) {
    color: var(--muted);
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
  .crumbs.whole .here,
  .crumbs.whole > :global(.views) {
    flex: none;
    overflow: visible;
  }
</style>
