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
tracks, ambient to techno, at a quarter and three fifths of each (2026-10-18). They are on **one scale** for the precheck,
the governor, the ratings and the thumbs (budget.ts): each budgeted meter over its budget at the music's activity then,
Wild at its middle, so a limit means the same on a quiet passage as on a loud one. On it, Standard's typical (its runs'
median mean) and most: motion 0.26 and 0.47, shape 0.16 and 0.42, colour 0.04 and 0.35, pumping 0.07 and 0.35; flashes
1 and 3 a second. The limits sit a quarter above Standard's most, up to the next 0.05 (motion 0.6, shape 0.55, colour
0.45, pumping 0.45; new looks 4 a minute; flashes 3), so Standard always passes (0 of 22 runs refused) and the wildest
tenth of recipes does not (48 of 440). The first baseline took the meters as they read (motion 0.9, shape 0.7, colour
0.4, pumping 0.4) while the governor scaled them by the music, so a quiet passage could hold Standard back.

**The owner's limits**, after: a rating page (Rate in the visuals' panel, or the visuals with `?rate=1`) shows two behaviours on the music that plays,
A for twenty seconds, then B, and asks which the owner prefers (A, B, the same, neither), keeping each choice with what
both did (their meters). Four kinds of pair in turn: a recipe against Standard, one recipe at both ends of one axis
(Flash, Speed, the fluid, Intensity, how long the lights ring), two recipes, Standard at both ends of an axis. About
twenty pairs, a quarter of an hour on the tablet. The ratings are kept on the server
(`/api/visuals/ratings`). Fitted (fit.ts) all together: each meter either does not matter to the owner or costs in
proportion to how far it goes over its limit, and the owner takes the cheaper side of a pair, or calls them the same
the closer they cost (Davidson's model of choices with ties, its tie rate fitted too); Neither is counted, not fitted. A
meter is given a limit only where that explains the answers better by more than a Bayesian information criterion's
penalty, each limit with the range the ratings cannot tell from it. Against a simulated owner (two meters that matter,
one strongly, ties among the answers; random pairs): a limit where there was none 0 to 2 times in 60; the strong limit
found in half the runs at 40 pairs and 17 of 20 at 150, the weak one in 4 of 20 at 150; a limit found held the true one
within its range 85 to 90 % of the time. So twenty pairs give a first look, not a fit; the one-axis pairs tell more than
random ones, and the thumbs go on from there. Flashes are not fitted: three a second is the rule.

### The runtime

**The governor** (governor.ts), in the visuals' loop: the meters stepped four times a second from what each frame did,
and for each kind of busyness a lever that holds it back while it goes over its budget and lets go once it is back
under. Pure; the loop applies its levers through the mapping (aspects.ts).

- **Flashes** (the rule, always on, never learned): over 2.5 a second on the quarter-of-the-screen rate (the meter reads
  in halves; WCAG's three with room to act before it), the Flash setting's swing closes (by half in about half a second)
  and its slow course is followed more slowly with it, so the swing shrinks and the course no longer rides each pulse.
  It opens again only after eight seconds without a reading over the rule: the meter tells of a burst a second late, and
  a passage that flashed comes back (with a shorter rest a lull between two left the next one's start unheld). In the
  lab, Flash at 200 % with the lights doubled stayed at three a second or under after the first trip; the first second
  of a session's first burst is the meter's own lag, and the precheck covers the dice before it.
- **Budgets:** motion holds back the turning's pace (and the fluid's), down to a quarter; shape churn the parts'
  evolving (their Evolution times up to four times as long) and Drift's walk; colour churn the hues' swing; zoom pumping
  the size's swing; new looks a minute the moments (their gap up to six times as long). A lever closes while its meter
  is over its budget (by half in about a second) and opens once it is under four fifths of it.
- **A budget** is a limit as Calm ↔ Wild and the music have it: limit × 2^(2·(Wild − ½)) × (½ + the music's activity),
  half as much to twice as much along Wild (at its middle, as the limits), less on a quiet passage, more on a loud one.
  The limits stand at the precheck's (DEFAULT_LIMITS) until the owner's ratings fit them, and as the thumbs teach them.
- **Wild** is a setting of its own (0 to 100 %, 50 % by default), shared in a sync group as the other behaviour settings.
- **A mark:** a small amber dot by Flash, Speed and Wild shows what the governor holds back now (on Wild, anything).
- **While the rating page runs a trial,** only the flash rule holds: the budgets are what the ratings are to tell. Each
  trial starts the meters and the governor anew, so one segment's flashes are neither counted nor held back in the next.
- **The rating page rates only what it should.** Each segment's first two seconds are not measured. The pair starts over
  from A where what it is compared on changed under it: another track; a seek (a jump over five seconds: a player's
  clock sets itself right by a second and a half); a look from outside (the loop counts each change of the look that is
  not Drift's walk or the Symmetry moments, such as a track's look, a recall, the group's, another colour mode, the
  cover's colours, and while one blends in, A waits, so neither side is measured mid-blend); the picture's size changing
  over a tenth; the page hidden or stalled over a second; a pause over five seconds (shorter ones are waited out). The
  run goes on where the visuals move (beside the page or over it) and ends as they close. While it runs, only Close,
  where they show, Full screen and the transport stay. Each side records its meters, the same on the one scale, what
  the loop knew (the music's activity, the flash rule's hold, the device's pace), its track and position; the record is
  version 4, with how often the pair started over after something had been measured.

**The dice** (Randomise, beside Another look): a recipe drawn and run against the playing track from where it is (the
precheck), a slice a frame, up to eight: the first within the limits as Wild has them (the flashes at three a second
whatever it says), else the one over them by least; without the track's signals yet, the first, the governor holding
it as it plays. Its behaviour and a new look go to the settings and the sync group. **Undo** puts the look, its pins and
the behaviour from before back, for the group too, until the next roll. In the lab a roll took about a quarter of a
second, and an undo restored every setting.

**The rating page** starts from **Rate** in the visuals' panel (the tablet's app has no address bar), or `?rate=1`; while
it runs, the controls that would change the look or the behaviour are away.

**Thumbs** (the owner: "I could get a general thumbs up or thumbs down for any visualization that I currently look at,
and it could then decide to tune"; "Simple is good, but we do need to handle the caveats"): two buttons above the track's
name. Each press is kept (`/api/visuals/thumbs`) with what the visuals did over the 20 seconds before it, for the
periodic fit; and it may move one limit at once, as an up-down staircase moves a threshold (Levitt 1971; Kaernbach's
weighted up-down, 1991), learning.ts:

- **Thumbs down** tightens the limit of the one kind of busyness that stood out most over those 20 seconds against what
  the defaults typically do, in the governor's own terms (each meter against its budget at a limit of 1), to a step under
  what it was then, so it acts at once. Where nothing stood out it only counts toward the fit: dislikes are pressed more
  often than likes, and many are of the look rather than its busyness, so each tightening something would ratchet
  everything calm.
- **Thumbs up** loosens, a step, the limit the governor holds the picture back by then (its amber dot): the way back out
  once a limit is tight. A system that learns only from what it shows narrows for good without one (Jiang et al., 2019,
  on recommender systems' degenerate feedback loops: continuous exploration is the remedy).
- **Steps** start at a tenth and halve each time a limit turns the other way, to a fortieth, so it settles.
- **Each limit drifts back** toward where it started, half way in thirty days: taste moves with mood and familiarity
  (Berlyne's inverted U, its optimum shifting with exposure).
- **None goes** under half or over twice where it started. The flash rule is not learned. **Forget** in the panel lets
  go of all of it.
- The learned limits are kept per user on the server (`/api/visuals/limits`, with a version: two devices learning at
  once each apply their press on the other's), loaded as the visuals open and applied at once, by the governor and the
  dice's precheck alike.

In the lab, against a stand-in server: a thumbs down while motion stood out (1.3 of the way from the defaults' typical
to their most) took its limit from 0.6 to 0.48 at once; a thumbs up while the governor held motion back loosened it a
twentieth, to 0.50 (the step halved as it turned); Forget let go of it on the server; both presses were kept with the
20 seconds before them.

## Consequences

- The stars stay as they were; the dice now change how the visuals follow the music too, within limits.
- Every behaviour shown can be measured the same way, live or simulated: the limits, the precheck, the ratings and any
  later report (a device telling the server what it shows) speak of the same numbers.
- The limits start from today's behaviour and become the owner's as the ratings come in, and the thumbs keep tuning
  them as the owner watches; undos and snapshots can refine them later.
- A limit learned is kept within bounds, drifts home, and is forgotten on one press: a bad evening cannot make the
  visuals dull for good, nor wild.
