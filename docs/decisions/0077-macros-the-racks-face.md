# 0077: Macros are the rack's face; the dice play the face

- Status: accepted (the owner, 2026-10-29: "One thing we should look at is the complexity and macros. Using the dice
  features, re-rolling things, it's all very convoluted. […] I believe macros are the key, but our macros do not
  properly work hierarchically"; then "Macros only" and "One level down")
- Date: 2026-10-29
- Builds on: 0057 (one engine: §6 devices and macros, §4d the dice, §7b2 pins), the owner's Macros module of
  2026-10-22 (µMAP style: a mapping sets its knob's base from→to, shown as a marker on that knob; the dice's Macros
  scope the default where a rack has macros)
- Supersedes: 0057 §7f's Track Look (the Dice on scope Everything)

## Context

Found in the code (2026-10-29), one of it by a test:

1. A device's own macros do nothing. Flatten lifts a device's modules, cables and pins, not its macros, so the engine
   never sees them: Bloom's Threshold at 0 and at 1 gave its inner threshold 0.6 both times.
2. No hierarchy. The rack's macros map straight onto knobs inside devices (Wild onto Calm's five Duckers); the shipped
   devices have no macros of their own; a rack macro cannot drive a device's macro.
3. A macro owns its knob unseen: the knob's own slider still shows and moves and does nothing; two macros on one knob,
   the last one wins.
4. The dice roll knobs, not macros: the panel's dice and a new track's look roll every unpinned knob, those a macro owns
   too (an invisible roll, which surfaces if the macro goes). The Macros scope exists; nothing defaults to it.
5. A macro's value is set at once where the one morph glides a knob's.
6. Six things make a knob jump (the dice, a module's Randomize, a category's, Reset to seed, a new track's look, a step's
   Next), and pins at four levels and the scope pins are the only defence: work is kept by pinning defensively.
7. Macros are made only while making a device in the graph; none can be added, changed or made for the rack after.

## Decision

**Macros are the rack's face: at every level, the few named controls that say what it does. The dice play the face.**
As Ableton's Racks: macros per rack, racks nested, the dice rolling macros only.

1. **Every level has its macros, and a macro reaches one level down.** The rack is the top device. Its macros map to
   its own modules' knobs and to its devices' macros; a device's macros map to its own modules. Nothing reaches past a
   device's edge: the rack's Wild drives Calm's Wild, which drives its five thresholds.
2. **A mapped knob shows it.** One macro a knob (a new mapping takes it over). The knob shows its macro and its range
   (the µMAP marker); it is the macro's, never a dead slider.
3. **The dice roll macros**, each within its range, at every level, what is pinned (`macro:<id>`) kept. The panel's dice
   and a new track's look (its seed the track's) both; a macro roll glides in the one morph. Everything and the
   categories stay under the dice's long press, for whoever wants them.
4. **What is not on a macro stays as it is set.** Pins become the exception: a macro pinned keeps its value.
5. **Macros are made where one is:** a knob's long press, Map to macro (its level's or its device's); a macro's page
   lists its knobs and their ranges. Existing pieces only (MenuSheet, pages).

## Consequences

- The shipped rack needs a face: macros that say what a look is, since the dice no longer roll the Flame's knobs one
  by one (below).
- The Flame's own checks on a roll (it must draw something: the survey) apply where a macro moves its look.
- The check against the track (the precheck's limits) stays with Everything and the categories; a macro roll stays
  within the ranges its maker chose.
- Kept racks and snapshots: Calm gets its own Wild, the rack's Wild maps to it; the shipped Dice rolls Macros.

## The shipped face (the owner, 2026-10-29: "Eight"; the dice roll "all macros")

- **Look:** Form (the Flame's form, its survey kept: it must draw something), Shape (Detail, Twist, Warp), Symmetry (and
  Mirror), Lens (its sides and depth), Colours (the four families of hues a track's look picks; Cover and Fire chosen
  by hand), Hue, Light (Lightness, Vividness), Effects (the effects' Amounts).
- **Behaviour:** Intensity, Accent, Speed, Wild.
- **The dice roll every macro** (the owner: "Yes, all macros"); a macro pinned is kept.

## The dice (the owner, 2026-10-29: "But keep the dice coherent too!!!")

- **The dice roll the face of where they are, from a seed:** on the rack its macros (and through them its devices');
  on a device's page its macros; on a module's page its knobs (a module's face is its knobs). The Dice module rolls the
  face of the level it sits in. A new track's look is the track's seed; a tap, a fresh one; Reset to seed, the track's
  again. Reach stays (how far a roll moves from where each control is); pins are the only exceptions.
- **The long press keeps the eleven scopes** (the owner: "Keep the 11 scopes"), beside the face.
- **No check against the track** (the owner: "No"): its eight tries and its second way of making the picture go; a
  macro's range and the Calm, live, keep a roll within bounds.

## Built (2026-10-29; nothing seen on a device yet)

1. A device's macros work: flatten lifts them (`<device>/macro:<id>`, a cable into one reaching its node); a macro maps
   its level's knobs and its devices' macros, the inner set by the outer. A macro reaching past a device's edge is
   moved into that device's macro of its name, mapped whole (macro-levels.ts): the rack's Wild maps Calm's Wild now.
2. A mapped knob shows its macro and the value it sets (µMAP's arc in the graph, the macro's name in the panel), never
   a dead slider.
3. The dice roll the face (every level's macros, pinned and mapped ones kept), surveyed where the look moves; a macro
   roll glides unless it maps a step; no check against the track. A kept rack: its new-track Dice on the face (rev 1).
4. The shipped face: the look's seven over the Flame (ranges as a track's look was drawn, through where the knobs
   are) beside Effects and the behaviour four; a kept rack gains them (rev 2).
5. A knob a macro owns rolls through its owner (a module's Randomize, the long press's scopes).
6. Macros made and changed in the panel: + New macro, a macro's page (value, pin, what it maps and each range, Map a
   knob, Remove), a knob's What moves it (its macro, Map it to one). Nothing jumps as a knob is mapped or let go.

Left: Map to macro from a knob's long press in the graph (the panel's What moves it has it); a macro's name changed.
