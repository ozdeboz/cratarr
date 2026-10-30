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

### The chrome around it, like Finamp's (decided 2026-10-14, after reading Finamp's source)

Finamp's chrome as read: `lib/components/now_playing_bar.dart` (the mini-player), `lib/main.dart` (the sidebar wraps
every page), `lib/screens/player_screen.dart` and `lib/screens/blurred_player_screen_background.dart`.

- **The navigation:** the sidebar from 1024 px stays as it is; below that the bar of pages moves from the bottom to
  the **top** (icons with labels, up to five, the rest under More), so the bottom edge is free for the mini-player.
- **The now-playing header above the page goes.** In its place, on Playlists and Incoming (on every page since
  2026-10-14, see the amendment below), a **mini-player along the
  bottom**, as Finamp's: a floating bar 64 px high, 12 px from the edges, corners 12 px; the cover with play/pause on
  it; the title (scrolling when it does not fit); artist and time (position / length); a heart (the favourite). The
  whole bar fills from the left as the track plays, in the colour taken from the cover. Tap or swipe up opens the full
  player, swipe left or right skips. **Swipe down does nothing** (in Finamp it stops playback and clears the queue;
  Cratarr is a remote across the room, and a stray swipe is not worth that).
- **On a phone the full player covers everything,** the navigation too; the chevron or a swipe down brings the page
  back. It slides up when it opens (300 ms) and fades when it closes, as Finamp's.
- **Wide windows show the sidebar by themselves** whenever something plays (Finamp's split screen), with no
  mini-player. Its chevron folds it to the mini-player until the bar is tapped (not remembered); maximise puts the
  player over the page and is remembered, as is the button that takes it back beside the page.
- **Toasts** float above the mini-player (or the bottom edge without it); beside the sidebar from 1024 px as before.
- **The full player takes its look from the cover:** the cover, heavily blurred under a dark veil (light theme: a
  light one), as its background, and its accent colour (progress, the play button's tint, the mini-player's fill)
  taken from the cover's most vivid colour and kept readable against the background; between tracks the colours fade
  over a second.

### Ported precisely (2026-10-14, owner: "port it more precisely")

The first version approximated Finamp; it is now a port of Finamp's `PlayerHideableController` with its default
settings (cover minimum padding 1.5 %, cover priority 8), in `lib/playerLayout.ts`:

- **Order of what gives way** (Finamp's `verticalHideOrder`): large gaps, the large play button, the player pill (in
  the place of Finamp's bottom-actions row, which Cratarr keeps because it holds the playlist count), small gaps, the
  info chips, the title's second line, the progress bar (its time labels go with it, as Finamp's slider).
- **The cover's allowance** per step is Finamp's: 1.5 % + `maxShrink` (1, 1, 2, 2, 3, 3, 4) × 8 × (the cover's size
  over 300 px, between 1 and 3) per side.
- **Spare height** as Finamp's `_controlsInternalHeight`: a third goes between the controls' three groups (track
  name; chips, progress and buttons; the actions row), up to 40 % of their height; of the rest a third goes under
  the controls and two thirds around the cover.
- **Widths:** portrait controls up to 1000 px, the buttons spread across them; landscape controls between 50 % and
  65 % of the width (at most 800 px), the cover on the left with 3 % of the height around it, the groups spaced
  4 / 4 / 10 / 4.
- **Orientation:** as Finamp's on the web: the larger cover wins, judged with the controls halfway between all shown
  and all dropped.

## Consequences

- The tablet in landscape gets a cover the height of the screen with the controls beside it, instead of a small one
  in a column.
- On a very small window the player loses detail in a known order before the cover becomes tiny; it scrolls only
  when even the title and play/pause no longer fit beside a minimal cover.
- The layout runs on every resize of the player; it measures a handful of elements and settles in at most seven steps.
- `docs/ui-conventions.md` describes the full-screen player this way from now on.

## Amendment (2026-10-14): the player on every page

The owner decided that the player belongs to the app shell on **every** page, not only on Playlists and Incoming:
with the local player (decision 0022) Cratarr plays music itself, so the player is a player in its own right, not
only a remote for adding what plays to playlists. The mini-player, the player beside the page and the player over
everything show on Repairs, Insights and Settings too. The server picker stays on Playlists and Incoming, the pages
that act on what a server plays.

## Amendment (2026-10-16): swipe up opens the queue

The owner: in the full player, over the page and beside it, swipe up opens the queue, not the playlists. The
playlists that hold the track are the top bar's add-to-playlist button. The output is the player pill over the title
(a button); the bottom row's output button is gone, its place kept for radio.
