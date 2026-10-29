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

- The shipped rack needs a face: macros that say what a look is (its form, shape, colours, light), since the dice no
  longer roll the Flame's knobs one by one. Which ones is the owner's to choose (open, below).
- The Flame's own checks on a roll (it must draw something: the survey) apply where a macro moves its look.
- The check against the track (the precheck's limits) stays with Everything and the categories; a macro roll stays
  within the ranges its maker chose.
- Kept racks and snapshots: Calm gets its own Wild, the rack's Wild maps to it; the shipped Dice rolls Macros.

## Open

- The shipped rack's look macros: which, and their ranges.
