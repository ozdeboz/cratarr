# 0066: One palette for both themes: a green, a red and an amber that hold on either page

- Status: accepted (the owner, 2026-10-24: "I am prepared to change primary color if we can find one that is identical
  in both light and dark"; "as close as possible to what we have and just passing WCAG"; then "Coral & honey")
- Date: 2026-10-24
- Builds on: 0063 (one health report: critical, warning), the dataviz method (categorical colours apart from status)
- Changes: web/src/app.css (the colour tokens), every component's coloured text, the logo and the app icons
  (tools/brand/record.py), the sign-in page, the Android theme, Insights' taste chart; Cratarr 0.8

## Context

The accent was a mint, `#3ddc97`, on the dark page and a darker green, `#12a36a`, on the light one. The mint is
1.8 : 1 on white, so the light theme needed its own; even that one is 3.2 : 1 there, short of 4.5 : 1 for small text,
and white labels on it were 3.2 : 1 too. The red and the amber also changed per theme (`#ff6b6b`/`#c62d2d`,
`#f5b942`/`#a86a00`), and Insights' chart used the dataviz reference palette, whose aqua, orange and yellow read as the
accent, an error and a warning.

No single colour can reach more than about 4.35 : 1 against both `#0f1115` and white: becoming lighter gains on one
side what it loses on the other. One colour for both themes can therefore pass WCAG for what an accent does (icons,
controls, large text: 3 : 1), not for small text (4.5 : 1).

## Decision

**One value per colour, for both themes**, each at least 3 : 1 on both pages and both cards (`#0f1115`, `#181b22`,
`#f5f6f8`, white):

| Token | Colour | Its label |
| --- | --- | --- |
| `--accent` | `#12a36a`: the nearest to the mint that holds on the light page (searched in OKLab) | `#06281a` (4.9 : 1) |
| `--danger` | `#c03e34` (coral): errors, critical, what removes | white (5.3 : 1) |
| `--warn` | `#c1811c` (honey): warnings | `#06281a` (4.9 : 1) |

The red sits at the dark end of what holds and the amber at the light end, beside the green: their lightness apart,
red and amber stay apart for colour-blind eyes too (the dataviz check: red against amber and both against the green).

**Text in a colour takes its ink**: `--accent-ink`, `--danger-ink`, `--warn-ink`, the same hue mixed toward the
theme's text until it reads at 4.5 : 1 or more. The accent's differs in the light theme only (it is 5.3 : 1 on the
dark page); the red's and the amber's are one mix in both. Icons, fills, borders and the logo keep the exact colour.

**Charts keep apart from the three**: Insights' taste over time draws four genres in a blue, a magenta, a violet and
an olive, each hue at least 35 degrees from the brand three, passing the dataviz checks on both pages; five could not
without one landing on a brand colour.

## Consequences

- Dark mode loses the mint's glow: the green is deeper.
- Small text in the accent, the red or the amber always reads, through its ink; a new coloured text uses the ink, never
  the bare token.
- No component writes a colour of its own: the mark's default fill, the icons (`tools/brand/record.py`, whose launcher
  template no longer writes the green in), the sign-in page and the Android theme use these values.
- A crate card's played meter is red, apart from the green ring of the open one.
