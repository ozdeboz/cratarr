<script lang="ts">
  // A panel's left part (0.9.3; the owner, 2026-10-25: Requests, Downloads, Insights, Health and Settings get "pure
  // breadcrumbs"): back, up one level, and its title as a breadcrumb. `trail` the levels above, the top first, each a
  // tap back there (without `go`, only said), then where one is.
  // One way on every device and every width (the owner, 2026-11-02: "there should never be a toggle between modes on any
  // device size. It should simply collapse the highest level that's no space for"): every level while they fit; short
  // of room, a group first (not a place: the owner, the same day, "Groups go first", so the level left of where one is
  // is always where Up goes), then the highest place, then the next, where one is last; only where one is, alone, is
  // cut with …. Up (↑, right of back) goes one level up the tree, so a level that went is one tap away all the same.
  // One rule for every panel (the owner, 2026-11-01): the levels are those Back walks through, each a tap back there;
  // only places are levels. Where one is can be a picker of its siblings (Settings' other parts, Health's other
  // problems: a move sideways instead of up and down again, as the Library's "Records ▾"). No levels above: only where
  // one is, as beside the page whose name its list already shows, and no Up.
  // A level above can be its group (the owner, 2026-11-02: Settings › Device › Appearance, the parts' picker was "way too
  // tall"): a picker of the groups, landing on the part last seen in one (lib/groupCrumb). Not a place: Back walks past
  // it, and so does Up.
  import GlassPicker, { type PickerOption } from './GlassPicker.svelte';
  import Icon from './Icon.svelte';
  import { rowPiece } from './glassRow.svelte';

  export type Siblings = { menuLabel: string; options: readonly PickerOption[]; chosen: string | null; onchoose: (id: string) => void };
  /** A level above: a tap back there (without `go`, only said), or its group's picker (`pick`). */
  export type Level = { label: string; go?: () => void; pick?: Siblings };
  let {
    trail = [],
    here,
    siblings = null,
    back = null,
  }: {
    trail?: Level[];
    here: string;
    siblings?: Siblings | null;
    /** Back's tap; none (beside the page's own list), no back. */
    back?: (() => void) | null;
  } = $props();

  /** Up: the nearest level above that is a place. */
  const up = $derived([...trail].reverse().find((l) => l.go) ?? null);

  /** A level's rank in the row (lib/glassRow): which goes first for want of room, the highest. Groups before places, the
   *  highest of each first (the owner, 2026-11-02: "Groups go first"); where one is last of all (10), cut before it
   *  goes (the title making way for the open field). */
  const rankOf = (i: number) => (trail[i].pick ? 30 : 20) + (trail.length - i);
</script>

{#if back || up}<span class="navpill crumbs-nav"
    >{#if back}<button aria-label="Back" title="Back" onclick={back}><Icon name="back" size={22} /></button>{/if}{#if up}{@const u = up}<button
        aria-label={`Up to ${u.label}`}
        title={`Up to ${u.label}`}
        onclick={() => u.go?.()}><Icon name="arrowUp" size={22} /></button
      >{/if}</span
  >{/if}<h2 class="crumbs">
  {#each trail as level, i (i)}<span class="level" {@attach rowPiece(() => ({ id: `crumb:${i}:${level.label}`, rank: rankOf(i), yields: true }))}
      >{#if level.pick}<span class="up pick"><GlassPicker shown={level.label} menuLabel={level.pick.menuLabel} options={level.pick.options} chosen={level.pick.chosen} onchoose={level.pick.onchoose} /></span>{:else if level.go}<button class="up" onclick={level.go}>{level.label}</button>{:else}<span class="up">{level.label}</span>{/if}<span class="sep" aria-hidden="true">›</span></span
    >{/each}<span class="here-box" {@attach rowPiece(() => ({ id: `crumb-here:${here}`, rank: 10, min: 48, yields: true }))}
    >{#if siblings}<GlassPicker shown={here} menuLabel={siblings.menuLabel} options={siblings.options} chosen={siblings.chosen} onchoose={siblings.onchoose} />{:else}<span class="here" aria-current="page">{here}</span>{/if}</span
  >
</h2>

<style>
  /* Back and Up on their glass (a panel's own bar); in App's row on a phone, its glass's own (lib/NavigationControls,
     which wins: these weigh nothing). */
  :where(.crumbs-nav) {
    display: flex;
    flex: none;
    padding: 2px;
    border-radius: 26px;
    background: var(--float-bg);
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
    box-shadow: var(--island-ring), 0 2px 12px rgb(0 0 0 / 0.3);
  }

  :where(.crumbs-nav) button {
    display: grid;
    flex: none;
    place-items: center;
    width: 44px;
    height: 44px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--text);
    cursor: pointer;
  }

  /* One line, the levels above muted; never wrapped. What it shows is its row's decision (lib/glassRow: a level there is no
     room for goes, groups first, where one is cut last); and whatever the decision, nothing of it is drawn past its
     glass: the line clips at its own edge (the owner, 2026-11-02: "This should not be possible"). */
  .crumbs {
    flex: 1;
    display: flex;
    align-items: baseline;
    min-width: 0;
    margin: 0;
    overflow-x: clip;
    overflow-clip-margin: 4px;
    font-size: 20px;
    font-weight: 600;
    white-space: nowrap;
  }

  .level {
    display: flex;
    flex: none;
    align-items: baseline;
  }

  .up {
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

  /* Where one is: the last to go, and alone the only part cut with …. */
  .here-box {
    display: flex;
    flex: 0 1 auto;
    min-width: 0;
  }

  .here {
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

</style>
