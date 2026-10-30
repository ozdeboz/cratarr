// The full player's layout (decision 0018), ported from Finamp's `PlayerHideableController`
// (lib/screens/player_screen.dart, branch `redesign`, read 2026-10-14) with its default settings. Finamp adds up fixed
// sizes per element; Cratarr measures its controls instead (`measure`), so the rules stay right when a font or a
// button changes. Everything else follows Finamp's code, step for step.

/** What gives way when the player runs out of height, first to last (Finamp's `verticalHideOrder`, without its bottom
 *  actions row; Cratarr's three bands, landscape's foot, before all): Cratarr's own row (menu, favourite, queue), the output pill (the only way to the output), the title,
 *  the artist line and previous / play / next never give way. Step n is dropped at level n + 1. */
export const STEPS = ['bands', 'gapsBig', 'buttons', 'gapsSmall', 'facts', 'title', 'progress'] as const;
export type Step = (typeof STEPS)[number];

/** Finamp's `maxShrink`: how far (percent of the cover's size, per side, times the cover priority) the cover may shrink
 *  so that this step can stay. */
const MAX_SHRINK: Record<Step, number> = {
  bands: 0,
  gapsBig: 1,
  buttons: 1,
  gapsSmall: 2,
  facts: 3,
  title: 3,
  progress: 4,
};

/** Finamp's defaults: `playerScreenCoverMinimumPadding` 1.5 (%), `prioritizeCoverFactor` 8. */
const MIN_PADDING = 1.5;
const PRIORITIZE_COVER = 8;
/** Finamp: portrait controls at most 1000 wide, landscape at most 800. */
const PORTRAIT_MAX = 1000;
const LANDSCAPE_MAX = 800;
/** The controls' narrowest sensible width (Finamp's unhideable elements plus the buttons: 144 + 14 + 96 px). */
const CONTROLS_MIN_WIDTH = 254;
/** The cover never gets smaller than this. */
export const MIN_COVER = 96;

export type Fit = {
  landscape: boolean;
  /** Steps 0..level-1 are dropped. */
  level: number;
  /** The cover's own square size. */
  cover: number;
  /** The box the cover sits in (centred, with Finamp's padding around it). */
  album: { width: number; height: number };
  /** The controls' box: the groups spread evenly inside it. */
  controls: { width: number; height: number };
};

type Measure = (landscape: boolean, level: number, controlsWidth: number) => number;

/** Finamp's `_controlsInternalHeight`: with room to spare a third of it goes between the controls' groups, until they
 *  have 40 % padding inside; the rest goes outside them. */
const internalHeight = (total: number, natural: number) => Math.min((total - natural) / 3 + natural, natural * 1.4);

function portrait(width: number, height: number, measure: Measure): Fit {
  const maxAlbum = Math.min(width, height);
  const controlsWidth = Math.min(width, PORTRAIT_MAX);
  const scale = Math.min(3, Math.max(1, (maxAlbum - 300) / 300));
  // Drop steps in order until the controls fit beside a cover shrunk by at most the next step's allowance.
  let level = 0;
  while (level < STEPS.length) {
    const padding = MIN_PADDING + MAX_SHRINK[STEPS[level]] * PRIORITIZE_COVER * scale;
    const target = height - maxAlbum * (1 - (padding / 100) * 2);
    if (measure(false, level, controlsWidth) < target) break;
    level += 1;
  }
  // The facts row (where the tape deck stands while it shows) gives way only where the controls would not fit even
  // beside the smallest cover (the owner, 2026-10-27: it went in a sliver of widths just before landscape, for some 55 px
  // more cover, and the deck then lay over the artist's line and the waveform); there the cover keeps the step below.
  const keep = STEPS.indexOf('facts');
  if (level > keep && measure(false, keep, controlsWidth) < height - MIN_COVER) level = keep;
  const natural = measure(false, level, controlsWidth);
  const desired = height - maxAlbum * (1 - (MIN_PADDING / 100) * 2);
  const padded = Math.max(natural, desired);
  const inside = internalHeight(padded, natural);
  // A third of the remaining padding goes under the controls, two thirds into the cover's region.
  const under = (padded - inside) / 3;
  const album = { width, height: Math.max(1, height - inside - under) };
  const pad = MIN_PADDING / 100;
  const cover = Math.max(MIN_COVER, Math.floor(Math.min(album.width * (1 - 2 * pad), album.height * (1 - 2 * pad))));
  return { landscape: false, level, cover, album, controls: { width: controlsWidth, height: Math.round(inside) } };
}

function landscape(width: number, height: number, measure: Measure): Fit {
  // Finamp: never more than 65 % of the width for the controls (unless the rest is only cover padding), at least 50 %.
  const desired = Math.min(Math.max(CONTROLS_MIN_WIDTH, width / 2), width - height);
  const maxWidth = Math.max(width * ((PRIORITIZE_COVER * 2 + 49) / 100), width - height);
  const minWidth = Math.max(CONTROLS_MIN_WIDTH, width * ((PRIORITIZE_COVER * 2 + 34) / 100));
  const target = Math.min(Math.max(desired, minWidth), Math.max(minWidth, maxWidth));
  const controlsWidth = Math.min(target, LANDSCAPE_MAX);
  let level = 0;
  while (level < STEPS.length && measure(true, level, controlsWidth) >= height) level += 1;
  const natural = measure(true, level, controlsWidth);
  const album = { width: Math.max(1, width - target), height };
  // Finamp's landscape cover: 3 % of the height around it (less on the side of the controls), then its own padding.
  const edge = height * 0.03;
  const inner = { width: album.width - edge - Math.max(0, edge - 20), height: height - 2 * edge };
  const pad = MIN_PADDING / 100;
  const cover = Math.max(MIN_COVER, Math.floor(Math.min(inner.width * (1 - 2 * pad), inner.height * (1 - 2 * pad))));
  // The controls as tall as the cover beside them (the owner, 2026-10-24: full screen in landscape they took less of the
  // height than they could; Finamp keeps them to 1.4 times their own), their groups spread over it; never less than
  // they need, never more than the height. Portrait keeps Finamp's (the owner: "perfect").
  const tall = Math.min(height, Math.max(natural, cover));
  return { landscape: true, level, cover, album, controls: { width: controlsWidth, height: Math.round(Math.max(internalHeight(height, natural), tall)) } };
}

/** The room each orientation has (its own padding off): asked of both, never of the one shown, so the choice never
 *  depends on what is shown (the tablet, 2026-10-28: each read the other's padding and chose the other, every frame). */
type Space = (landscape: boolean) => { width: number; height: number };

/** Lays the player out in the room `space` gives. On the web Finamp picks the orientation that gives the larger cover,
 *  from an estimate of the controls (halfway between all shown and all dropped). */
export function fit(space: Space, measure: Measure): Fit {
  const p = space(false);
  const l = space(true);
  const estimate = (measure(false, 0, Math.min(p.width, PORTRAIT_MAX)) + measure(false, STEPS.length, Math.min(p.width, PORTRAIT_MAX))) / 2;
  const landscapeWidth = Math.max(CONTROLS_MIN_WIDTH, l.width / 2);
  const landscapeAlbum = Math.min(l.width - landscapeWidth, l.height);
  const portraitAlbum = Math.min(p.width, p.height - estimate);
  return landscapeAlbum > portraitAlbum ? landscape(l.width, l.height, measure) : portrait(p.width, p.height, measure);
}
