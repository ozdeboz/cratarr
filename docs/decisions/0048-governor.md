# 0048: Visuals: a complete randomisation, kept watchable

- Status: accepted (the owner, 2026-10-18: random behaviour, not only looks, for the release after group sync and the
  fluid; "Small fix in 62, big work in 63, and the governor and the groundwork related to that in 64")
- Date: 2026-10-18
- Builds on: 0046 (the visuals, their routes and settings), 0047 (the fluid)

## Context

The stars (Another look) give another look at random: the flame's shape and colours, nothing of how it follows the
music. The owner wanted that kept exactly as it was, and beside it a complete randomisation: another way of following
the music too (its routes, how strongly it flashes, how fast it turns, the fluid, the grain, how long the lights ring),
on the dice. Routes drawn at random can be a mess: everything flashing on every hat, the picture spinning, the colours
churning. So the randomisation is held to limits, and the limits are the owner's, not guessed.

## Decision

**A recipe** (web/src/lib/visuals/recipe.ts) from a seed, the same on every device: the backbone that makes the
visuals follow the music at all (loudness on size, colour and turning; beats on glow and size; each band's level on its
part's shape, turn and tint; its hits on its light), each amount varied around today's, and 3 to 6 more routes from 24
that make musical sense, no two of one kind; light and glow together capped; Flash, Speed, Intensity, the fluid, the
grain and the bands' times varied within watchable ranges. The look, the pins and what the device can afford are not
the recipe's.

**Measured** (meters.ts), one definition for everything that judges the visuals, from what each frame did:

- **Flashes** as WCAG 2.2 counts them (2.3.1): a pair of opposing changes in relative luminance of 0.1 or more, the
  darker under 0.8; a red flash a pair into and out of a saturated red moving its chromaticity over 0.2 (CIE 1976 u'v').
  The picture is read back by tiles after each frame (flashmeter.ts: a blit, each tile's mean in linear light, a pixel
  buffer and a fence, so it never stalls; about 0.3 ms a frame on the tablet).
- **Over how much of the screen** is the owner's: "Full screen flashes at most 3 times per second, but if there is some
  minor flashing deep inside a cloud, it shouldn't be limited", and of the areas offered, a quarter of the whole screen.
  WCAG's own area (a quarter of any 10° of view, about a third of the screen across and down) is measured too and
  reported, not held: by it, today's beat- and hat-lit parts pass three a second on rhythmic music (4 to 8 in the lab).
- **Motion** (the parts' turning), **shape** and **colour churn**, **zoom pumping**, **new looks a minute** (the dice's
  and the moments', not a track's own).

**A precheck** (precheck.ts): before the dice show a recipe, it is run for half a minute from where the track plays,
through the visuals' own mapping (aspects.ts) on the track's own signals (musicOf), and measured; one over the limits is
refused and another drawn. No picture: the flashes are judged from the brightness the whole picture would pulse with.
About 25 ms a run on a PC, done in slices so a tablet keeps its frames.

**The limits**, to start (DEFAULT_LIMITS), from a baseline: Standard (production as it is) and 20 recipes over 11 varied
tracks, ambient to techno, at a quarter and three fifths of each (2026-10-18). Standard's typical and most: motion 0.40
and 0.69, shape 0.32 and 0.55, colour 0.08 and 0.34, pumping 0.14 and 0.32, flashes 1 and 3 a second. The limits sit a
little above Standard's most (motion 0.9, shape 0.7, colour 0.4, pumping 0.4, flashes 3), so Standard always passes
(0 of 22 runs refused) and the wildest tenth of recipes does not (50 of 440).

**The owner's limits**, after: a rating page (the visuals with `?rate=1`) shows two behaviours on the music that plays,
A for twenty seconds, then B, and asks which the owner prefers (A, B, the same, neither), keeping each choice with what
both did (their meters). Four kinds of pair in turn: a recipe against Standard, one recipe at both ends of one axis
(Flash, Speed, the fluid, Intensity, how long the lights ring), two recipes, Standard at both ends of an axis. About
twenty pairs, a quarter of an hour on the tablet. The ratings are kept on the server
(`/api/visuals/ratings`). Fitted (fit.ts) all together: each meter either does not matter to the owner or costs in
proportion to how far it goes over its limit, and the owner takes the cheaper side of a pair; a meter is given a
limit only where that explains the choices better by more than a Bayesian information criterion's penalty, each limit
with the range the ratings cannot tell from it. Against a simulated owner: no limit where there was none (0 in 20
runs), the true limits within the ranges (15 of 15); twenty random pairs give rough limits, the one-axis pairs tell
more. Flashes are not fitted: three a second is the rule.

### The runtime

(The governor in the visuals' loop, its budgets and levers, the flash limiter on the quarter-of-the-screen rate, Calm ↔
Wild, and undoing the last roll: added with that side.)

## Consequences

- The stars stay as they were; the dice now change how the visuals follow the music too, within limits.
- Every behaviour shown can be measured the same way, live or simulated: the limits, the precheck, the ratings and any
  later report (a device telling the server what it shows) speak of the same numbers.
- The limits start from today's behaviour and become the owner's as the ratings come in; the governor's own log, and
  what the owner undoes or keeps as a snapshot, can refine them later.
