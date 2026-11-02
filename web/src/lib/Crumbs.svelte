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
  import { tick } from 'svelte';
  import GlassPicker, { type PickerOption } from './GlassPicker.svelte';
  import Icon from './Icon.svelte';

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

  let line = $state<HTMLElement>();
  /** The levels above that have gone for want of room: groups first, then places, the highest first. */
  let gone = $state<ReadonlySet<number>>(new Set());
  /** Each level's own width (with its ›) and where one is's, as laid out whole; measured again as they change. */
  let widths: number[] = [];
  let measuredFor = '';

  /** The room the line has: its own, and what the row it is in leaves free (the glass beside back is only as wide as
   *  what it holds; a bar's line takes the rest itself). */
  function room(el: HTMLElement): number {
    const row = el.closest<HTMLElement>('.page-row, .bar');
    if (!row) return el.clientWidth;
    const s = getComputedStyle(row);
    const kids = [...row.children].filter((c): c is HTMLElement => c instanceof HTMLElement && c.offsetParent !== null && getComputedStyle(c).position !== 'absolute');
    const used = kids.reduce((n, c) => n + c.offsetWidth, 0) + Math.max(0, kids.length - 1) * (parseFloat(s.columnGap) || 0);
    const free = row.clientWidth - (parseFloat(s.paddingLeft) || 0) - (parseFloat(s.paddingRight) || 0) - used;
    return el.clientWidth + Math.max(0, free);
  }

  function fit() {
    const el = line;
    if (!el) return;
    const key = `${trail.map((l) => l.label).join('\n')}\n${here}`;
    if (key !== measuredFor) {
      // Laid out whole for a moment (the same frame, nothing painted): every part at its own width.
      el.classList.add('measuring');
      widths = [...el.children].map((c) => {
        const m = getComputedStyle(c);
        return (c as HTMLElement).offsetWidth + (parseFloat(m.marginLeft) || 0) + (parseFloat(m.marginRight) || 0);
      });
      el.classList.remove('measuring');
      measuredFor = key;
    }
    const have = room(el);
    let need = widths.reduce((n, w) => n + w, 0);
    const order = [...trail.keys()].sort((a, b) => Number(!trail[a].pick) - Number(!trail[b].pick) || a - b);
    const out = new Set<number>();
    for (const i of order) {
      if (need <= have + 0.5) break;
      need -= widths[i];
      out.add(i);
    }
    gone = out;
  }

  $effect(() => {
    void trail;
    void here;
    tick().then(fit);
  });
  $effect(() => {
    const el = line;
    if (!el) return;
    const row = el.closest('.page-row, .bar');
    const seen = new ResizeObserver(() => fit());
    if (row) seen.observe(row);
    addEventListener('resize', fit);
    return () => {
      seen.disconnect();
      removeEventListener('resize', fit);
    };
  });
</script>

{#if back || up}<span class="navpill crumbs-nav"
    >{#if back}<button aria-label="Back" title="Back" onclick={back}><Icon name="back" size={22} /></button>{/if}{#if up}{@const u = up}<button
        aria-label={`Up to ${u.label}`}
        title={`Up to ${u.label}`}
        onclick={() => u.go?.()}><Icon name="arrowUp" size={22} /></button
      >{/if}</span
  >{/if}<h2 class="crumbs" bind:this={line}>
  {#each trail as level, i (i)}<span class="level" class:gone={gone.has(i)}
      >{#if level.pick}<span class="up pick"><GlassPicker shown={level.label} menuLabel={level.pick.menuLabel} options={level.pick.options} chosen={level.pick.chosen} onchoose={level.pick.onchoose} /></span>{:else if level.go}<button class="up" onclick={level.go}>{level.label}</button>{:else}<span class="up">{level.label}</span>{/if}<span class="sep" aria-hidden="true">›</span></span
    >{/each}{#if siblings}<GlassPicker shown={here} menuLabel={siblings.menuLabel} options={siblings.options} chosen={siblings.chosen} onchoose={siblings.onchoose} />{:else}<span class="here" aria-current="page">{here}</span>{/if}
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

  /* One line, the levels above muted; never wrapped. */
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

  .level {
    display: flex;
    flex: none;
    align-items: baseline;
  }

  /* A level there is no room for: gone, the highest first (Up still reaches it). */
  .level.gone {
    display: none;
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
  .here {
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Measured whole for a moment (fit): every part at its own width. */
  .crumbs:global(.measuring) > * {
    display: flex;
    flex: none;
  }

  .crumbs:global(.measuring) .here {
    overflow: visible;
  }
</style>
