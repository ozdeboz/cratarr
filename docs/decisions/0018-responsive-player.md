# 0018: The full-screen player fits itself to every size

- Status: accepted
- Date: 2026-10-14
- Builds on: 0006 (players); changes the full-screen player in `docs/ui-conventions.md`

## Context

Cratarr's player is a remote for a player elsewhere (Finamp on the tablet). Full screen it is one centred column at
every size: cover, title, artist · album, controls, progress, favourite and "in N playlists". When height runs short
only the cover shrinks (down to 96 px, then the panel scrolls); nothing gives way, nothing reorders, and a short,
wide window (the tablet in landscape, a desktop) gets a small cover with empty space beside it.

Finamp's player (read in its source, `lib/screens/player_screen.dart` and
`lib/components/PlayerScreen/player_split_screen_scaffold.dart`, branch `redesign`, 2026-10-14) does this well with a
space budget rather than breakpoints: each optional element has a size, a fixed order decides what gives way first,
the cover may give up a share of its size to keep an element (a larger share the larger the cover), the layout that
gives the larger cover wins, and wide windows show the player beside the page. Cratarr takes the ideas, not the code
(Finamp is MPL-2.0 and Flutter; this is written from the description).

## Decision

The owner chose each point (2026-10-14).

- **What gives way, first to last:** generous gaps, large transport buttons, the player pill, the title's second
  line, the time labels, the progress bar. **Never:** the title, play/pause, and "in N playlists" (the reason this
  screen exists in Cratarr). Artist · album, previous/next and the favourite stay too.
- **Measured, not hand-tuned:** the player measures its own controls at each step (`lib/playerLayout.ts`) instead of
  keeping a table of sizes, so a font or button change cannot make it wrong.
- **The cover negotiates:** an element stays while keeping it costs the cover at most its allowance, a share of the
  largest possible cover that grows from 1× at 300 px to 3× from 900 px. On a phone the cover mostly wins; on a
  tablet or desktop more controls stay.
- **Portrait or landscape:** both are worked out and the one that gives the larger cover is used (cover left,
  controls right in landscape). Device orientation is not consulted, so a split window or a near-square one gets the
  better of the two.
- **Spare room** goes evenly around the cover and the controls; it never stretches the controls apart.
- **Wide windows get a sidebar:** when the content area is at least 800 × 500 px, full screen puts the player beside
  the page (at least 275 px for the player, 400 px for the page), resizable by dragging its edge, the width remembered
  by the browser. The same layout logic runs inside it. A maximise button gives the old full screen over the page;
  from there a button returns to the sidebar. While the sidebar shows, the header above the page does not.
- **The cover's gestures** in full screen: tap plays or pauses, double-tap toggles the favourite, swipe left or right
  skips to the next or previous track, swipe up shows the playlists (closes full screen over the Playlists page).
  Swipe down still closes it.

## Consequences

- The tablet in landscape gets a cover the height of the screen with the controls beside it, instead of a small one
  in a column.
- On a very small window the player loses detail in a known order before the cover becomes tiny; it scrolls only
  when even the title and play/pause no longer fit beside a minimal cover.
- The layout runs on every resize of the player; it measures a handful of elements and settles in at most seven steps.
- `docs/ui-conventions.md` describes the full-screen player this way from now on.
