# Interface conventions

How Cratarr's pages are built and worded. Every change to the interface follows this; a change that needs an
exception changes this file first. Agreed with the owner on 2026-10-12.

## Structure

- **Pages:** Library (decision 0034: Incoming and Collection as one page; `#/incoming` and `#/collection` open it), Crates, Playlists; Insights, Health and Settings apart
  (at the bottom of the sidebar, in that order; on a phone they follow the others in the same order). One thing lives in one place: no setting, list or action
  appears on two pages. Health lists the library's files that cannot be read properly; playlists are repaired in the
  panel's Repair tab (decision 0025).
- **No page titles.** The navigation shows where you are. A page starts with its toolbar, then its content.
- **Navigation:** from 1024 px a sidebar on the left; below it a bar of pages fixed along the **top** (icon and label,
  up to five, the rest under More).
- **The player is part of the app shell** (decision 0018, after Finamp), on every page: since Cratarr plays music
  itself (decision 0022) it is a player, not only a remote for adding what plays. Which server a browser follows is
  chosen in the filter sheet (the funnel in the floating pill: a card per server), not on the pages; no choice is
  Cratarr's own source where the server has it.
  - **Mini-player** (`lib/MiniPlayer.svelte`): a bar floating along the bottom, 64 px high, 12 px from the edges,
    corners 12 px: the cover with play/pause on it, the title (scrolling when it does not fit), artist and time, and
    the heart (the player's account's Jellyfin favourite). The bar fills from the left as the track plays, in the
    cover's colour. Tap or swipe up opens the full player; swipe left or right skips; swipe down does nothing.
  - **Full player** (`lib/NowPlaying.svelte`): the cover (library cover, else the album image from the server), the
    active player named in a small floating pill over the title (even when there is only one; tap: the output's sheet,
    the only way there), title, artist · album, the transport controls (shuffle, previous, play/pause as a rounded
    square, next, repeat), progress, and the bottom row (menu, heart, queue with its place under it; the label hangs
    below its icon so the icons line up). The top bar: close at the left; add to playlist (lit while the track is in a
    playlist), the sleep timer (lit while one runs) and beside / over the page at the right, icons only. It lays itself out
    for its space (`lib/playerLayout.ts`, a port of Finamp's layout): portrait or landscape, whichever gives the
    larger cover; spare room spreads its three groups (track name; chips, progress and buttons; the actions row);
    when short of height the large gaps, the large play button, the small gaps, the info chips, the
    title's second line and the progress bar give way, in that order. Playing on this device the progress bar is a
    slider (Finamp's): drag the handle and the time follows, the player seeks on release; arrow keys move 5 s. Its background is the cover, blurred under a veil; its accent comes from the cover. On the
    cover: tap plays or pauses, double-tap is the favourite, swipe left or right skips; anywhere (over the page and beside it): swipe up opens the queue, swipe down closes.
  - **Where it shows:** with room for it (a content area of 800 × 500 px or more) the full player sits beside the page
    by itself while something plays, resizable at its edge (handle in the middle), the width remembered; its chevron
    folds it to the mini-player until that is tapped. Otherwise the mini-player shows, and opens the full player
    **over everything**, navigation included (slides up, fades out); its chevron, Esc or a swipe down close it, and
    so does changing page. Maximise (beside the page) and back-beside-the-page (over it) are remembered.
  - **Queue** (`lib/QueueSheet.svelte`, as Finamp's queue sheet): a sheet over the player on the blurred cover, a grab
    handle and "Queue" (drag it down to close), and in its row the tools (decided 2026-10-16, as Symfonium's queue):
    search (in place: the row grows into the field, as the pages' search; the rows whose title, artist or album match
    show) and, where the page holds the queue, Clear (what comes next goes; the toast's Undo puts it back); not shuffle
    and repeat, which are the player's (the same twice was one too many). **More music** floats over the list's lower
    right corner where the page holds the queue: a pill saying what is on ("Radio · ∞ Same era", else "Off"), lit in
    the cover's accent while either is, opening its sheet (the player's radio button too): Radio, after each of the
    queue's own tracks (Off, Similar to the track, Same artist, Same era; how many after each on a slider, 1 to 10, set when the finger lifts), and Autoplay, when the queue ends (Stop, Similar, Same artist, Same era,
    Random; Queue albums, Restart playlists), and for both Ignore favourites (hearted tracks left out of their picks),
    each a choice of one with round marks as View's order (decision 0036, amended); the previous tracks folded under "Previous
    Tracks" (this device only); the playing track as a card pinned at the top that fills in the cover's colour as it
    plays, with play/pause on its cover, the heart and the track menu; what it plays from as a chip that opens it (its
    album, playlist or crate), or "Playing on …" for the server's player, and "i of n · time left · until 05:09" (when
    it ends on the clock) pinned under it; then what comes next, 70 px rows (60 px cover, title up to two lines, the
    artist and the album, not the album the queue plays, Radio's or Autoplay's mark before them; length, heart, ⋮). A tap plays
    a row; a row's cover held plays its track from a third of the way in until the finger lifts, over what plays,
    which then goes on where it was (decided 2026-10-16; this browser's own stream for now). Playing on this device a row's handle reorders and a swipe left past 65 % removes it,
    with no undo, as Finamp; the server's player takes neither command. A long-press or right-click opens a menu, the
    same everywhere in the sheet: on the playing track's card (it has no ⋯) the playing track's own menu; on a row a
    menu about its place in the queue: Play now and Play next (moving it there on this device; "Play again next", a
    copy, on the server's player), then Add to playlist, the favourite and Remove from queue (this device).
  - **Output** (`lib/OutputSheet.svelte`, decisions 0019 and 0022): the speaker button in the actions row (menu,
    heart, count, output, queue) opens a "Play on" sheet: This device, and the server's player by its name, the
    current one checked. Choosing the other one moves the music there (the queue and the position) and stops the side
    that played; nothing asks first, the player simply carries on there. The choice is remembered by the browser.
    Whichever side plays, the player looks and works the same; playlists, the heart and the info chips act on the
    track that plays there. A track this device cannot play is skipped with one error line naming it.
  Pages never show their own now-playing; they read the shell's shared state (`lib/live.svelte.ts`,
  `lib/remote.svelte.ts`).
- **Widths:** grids (the playlist chips) use the full width; lists and forms stop at 900 px, left-aligned.
- **A viewed playlist** (decision 0023, `lib/PlaylistView.svelte`): in the panel beside the page, or on a phone a
  full-height layer with the mini-player floating over its end. It shows the tracks; the selection's (one playlist, or
  several ticked with Select) **other views** (decision 0025) are entries of its ⋮ menu since 2026-10-16, no tabs row:
  Info (what Insights knows of it: how focused, its closest playlists), Export…, Repair…, Sync…. Each opens as a step
  of its own (back returns to the tracks, as does Esc) under a plain bar: back, the view's name over the playlist's,
  the panel's place; a view with nothing to do says so. Several
  playlists: their covers and count in the header, ✕ alone in the bar, and on Tracks a folding section per playlist
  (read-only). ✕ (and Esc, Back) closes the panel: the player folds to the mini-player rather than showing in its
  place (decision 0025). Its header is Symfonium's album screen (2026-10-15): the cover edge to edge (a playlist's
  mosaic; a crate's band with its icon), under **two floating pills** (back, when the panel has a view before this
  one, and ✕; the filter, View (the tracks' order and direction, as their header's), where it plays, and the panel's
  place; back and ✕ are rounds as tall as the pill), and at the cover's foot the **action bar**, icons only: Play, Resume (an album of Cratarr's own
  library: where it was left), Shuffle, Play next, Add to queue, then the subject's own (the heart; a playlist's pencil
  on Tracks and menu; an album's menu, the one every album has (below); a crate's favourites toggle, pencil
  and Make playlist). Scrolled up to the top the action bar stays there, and the pills float just under it, over the
  list. The pills are the panel's (not the pages'), so the player covers them once open. Under the cover the name large, one line of what it is (an album: length,
  year, format, bitrate; otherwise how many tracks and the length), an album's artist as a chip, the genres. Over the
  tracks **their order** (the subject's own, or by title, artist, album, length; a playlist's also by album artist, year,
  track number, play count, last played and when an entry was added, for entries added since 2026-10-16; a tap chooses, the arrow turns it
  round), how many, and **select mode** (a long-press on a track, or the icon: circles to tick, and a bar floating over
  the list's end plays, puts next, queues or favours the ticked tracks, and in the app downloads them). An album's rows have no cover: the number,
  the playing one's dots instead; every track's row ends in ⋮ (its menu). **An album ends** as Symfonium's (decided
  2026-10-16): More from the artist (their other albums on that server, newest first, in a row that scrolls sideways;
  a cover opens that album, the heading's › goes to the artist), then About: genres and year as chips, last played and
  date added (date and time), and the media provider (the server and library, with the library's dot); after Year its
  rating as five stars, the same as its menu's (a tap rates). Anything not known is left out. The album's artist chip opens the artist too.
  **An artist** (decided 2026-10-16, as Symfonium's; `?view=artist:<server>/<name>`, by name as the albums credit
  them) is a subject of the panel like an album: their newest album's cover (no artist photos yet), the action bar
  (Play, Shuffle, Play next, Add to queue: all their tracks, album by album, newest first; Start radio on Cratarr's own
  player; the heart), the name and "8 albums • 124 tracks • 7:06:37". Then Albums (theirs, newest first) and Appears
  on (others' albums they are credited on a track of), both as covers three across on a phone, and Top tracks: their
  five most played (by your plays), whose › shows all their tracks most played first (`artist-tracks:`). Their rows say
  album and year rather than artist. Go to artist (an album's or a track's menu, More from's ›, the chip) and a cover
  in these rows open over what the panel views: a step of its own, which back returns from. A list sorted the other
  way round keeps equals in the subject's order. In
  edit mode the cover and the name stay where they are; the action bar's place holds Cancel (a reset arrow, red once
  something changed) against Save (an arrow down to a line on a filled accent round), and the pills keep ✕. Rows as the
  queue's; edit mode as Finamp's playlist edit screen: the title is editable where it is, looking the same (Spoken
  as… is in the playlist's menu); rows with handle and ✕, swipe to
  remove, "Removed tracks" to restore; all written once on Save, a rename last (the panel follows the new name).
- **New playlist, New crate** are the panel's edit mode with nothing made yet (decided 2026-10-16: "creating a playlist
  should look like editing one, with regard to everything"), from the + at the end of the page's pill
  (`lib/TopPill.svelte`, `pageView.usePageAction`): the name editable where it shows ("New playlist", "New crate" until
  typed), the caret in it at once, Cancel and Save (once it has a name) in the action bar's place. A new playlist is
  made by its Save (`?view=playlist-new`); a new crate is the builder, saved with its Save.
- **A menu's head and actions** (`lib/MenuSheet.svelte`, decided 2026-10-16, as Symfonium's): a small cover (64 px),
  the title large beside it and what it is under that in one muted line, on the sheet itself (no card); a track's,
  an album's heart at the end. The large actions are icons only (their words on a long-press, as every icon
  button's), Play in a ring of the cover's accent.
- **One menu per album** (`lib/AlbumMenu.svelte`, decided 2026-10-16, after Symfonium's album sheet), wherever it is
  opened (a Library row's ⋮, the select bar's More with one ticked, the album panel's ⋮): the album at the top with
  its heart, the large actions (Play, Resume when it has a resume point, Shuffle, Play next, Add to queue), then
  entries on rounded cards: View (not from the panel showing it), Go to each of its artists (the source's list of
  them, else the credit), Go to its genres (two at most); its rating (five stars, when its server keeps ratings; a tap
  on the rating it has takes it off); Add to playlist, Download (in the app), Share; what its library allows
  (decision 0034: Move to each other used library, Replace in Music for an upgrade, Delete, a second step when it is
  in Music, Cancel conversion while queued), each question asked in the sheet; Change cover on this device (a
  picture kept in this browser, shown wherever the album's cover is) and Read the cover again (or Use the server's
  cover); Remove resume point, Web search, Genius search.
- **An artist's menu** (`lib/ArtistMenu.svelte`, the artist panel's ⋮, decided 2026-10-16) as the album's: the artist
  with their heart, the large actions, Start radio, Add to playlist (all their tracks), Download, Share, and their
  picture on this device. No Instant mix (the owner) and no rating.
- **Keep on this device** (decision 0038, in the app): a switch entry (`MenuEntry`'s `switched`: the entry with a
  switch at its end) in the album's, the artist's and the panel playlist's menu, beside Download (a one-time copy);
  on a crate or a genre, an icon in its action bar, in the accent while kept.
- **Add to playlist** (a track's, an album's, an artist's menu; `lib/PlaylistToggles`): the playlists as toggle cards
  with the size buttons beside the heading (decided 2026-10-16): two bare icons, dimmed when they would change
  nothing; one column by default, two when smaller, never three on a phone (three only on a wider sheet; a name wraps
  to two lines); over them the lists' header (Name, Tracks, Ticked first, either way round; Ticked first by default).
  Size and order are remembered apart from the Playlists page's; the orders are the page's (one function).
- **Share** (track, album, one playlist): the item in words ("Artist – Title") and its link here, through the
  phone's share sheet (the app's in the app), else copied.
- **A favourite album's cover** carries a small heart in its upper right corner, in the Library's rows and tiles.
- **Downloads** (in the app) is the Library's list of what is on this device (decided 2026-10-16: "a special case of
  the library page"): the same rows or covers, order, size, search and select, each album as the Library lists it
  (else as its download says), the count with the space they take ("3 albums · 225 MB"). A cover plays what is on the
  device, so the page plays without a connection; an album still coming shows how far ("Downloading 6 of 10"), one
  partly here "3 of 8 on this device". The select bar plays or removes; the menu is the album's one menu, with Remove
  from this device (and Download for the rest), as everywhere an album is downloaded.
- **Crates** (decision 0024, between Incoming and Playlists): the permanent crate (Unlisted) and one's own as **big
  cards** (decided 2026-10-16, `lib/CrateCard`; one across a phone): its picture on the left (its own, else a mosaic of
  its albums' covers), then its name, tracks and length with the favourites setting's small heart, its top three
  genres, "312 unplayed" and "+12 new" (this week's) as badges, and along its foot how much of it was played (the
  explored bar). Tinted as the playlist chips, the viewed one bordered. The same picture is the crate's cover in the
  panel. Its edit mode has Change image (and Remove image) in the middle of the cover, set with Save as the rest. A
  crate is viewed in the panel as a
  playlist does, without the statistics and editing, with Play, Shuffle and its favourites toggle (No favourites,
  Only favourites, All tracks; decision 0024, amended) as its actions; its rows come in pages, newest first; the arrow-into-a-box icon
  (where a playlist has the pencil) makes a playlist of it, which is then viewed. "New crate" (and the pencil on one's own
  crate) opens the crate's **edit mode** in its own panel, as a playlist's: the name editable in place (the pencil
  beside it), the count under it, and the **crate editor** where the tracks were: conditions as cards (the axis, an
  is/NOT toggle, its values as chips, a range as two numbers, a yes/no axis as just its name; negated cards tinted
  red), "Add condition" offering the server's axes, and below the first tracks. The bar: Delete (one's own crate; a
  second tap confirms), Cancel in the pencil's place, the favourites toggle, Save. The draft is kept in the browser
  until Save or Cancel. A genre (a track's Go to genre, a genre on top of a list) opens as a crate of that one genre.
- **Playlist chips** (the Playlists page, medium rows: see Rows) look like Finamp's add-to-playlist entries: tinted in the accent, 10 % when
  the playing track is not in the playlist and 30 % when it is; the name (two lines at most), its marks, the count, a
  divider and Finamp's plus circle or filled check circle. A tap on the circle adds or removes the playing track; a
  tap on the label views the playlist (decision 0023). **Select** (decision 0025, in the toolbar) makes a tap on the
  label tick the playlist instead (the panel shows the ticked ones beside the page; on a phone View N, an icon);
  the circle keeps adding or removing the playing track; with playlists ticked, a ⋮ at the pill's end (where
  + New playlist is otherwise) is the playlist menu for them (the same menu as a long-press; for several without
  Spoken as… and Rename). A ticked playlist has the border of the one viewed in the
  panel: one state, one look. A favourite playlist has a small red heart (Finamp's list mark)
  above the problems' dot, in the dot's place, so the chip gets no wider. The playlist viewed in the panel has a border
  in the accent and no extra fill: fill means "holds the playing track", the border "is shown in the panel". When the playing
  track is outside Music (it cannot go in playlists), the chips it is not in are quieter and their plus circle is
  dashed (Finamp's mark for a playlist it cannot add to); a tap says why.
- **Pinned playlists** (the playlist menu: Pin to top / Unpin) come first on the Playlists page, in the order they
  were pinned, marked with a small pin after the name; "✓ first" applies within the pinned and within the rest. Pins
  are remembered by the browser, like the sort order and size; a rename keeps the pin.
- **A page's floating row** (2026-10-15, `App.svelte`, `lib/TopPill.svelte`): back on the left (a round as tall as the
  pill; nothing there while there is nowhere to go back to: no mark in its place, 2026-10-16), and on a page with something to
  play the pill on the right: **search** (a magnifier that opens into a field over the row, the whole row on a phone, at
  most 520 px on a wide page; ✕ empties and folds it, Esc folds it, the magnifier stays in the accent while a search
  applies; each page starts with it empty), the **filter** sheet, **View** (the list's order and direction, as its
  header's; a playlist's size; albums' rows or covers and size), and where it plays when nothing else leads there.
  Last in the pill the page's own button where it has one: a + where it makes something (New playlist, New crate),
  (what is ticked has the select bar, lib/SelectBar). The floating controls (the pill, back, ✕, the select bars)
  share one surface: the mini-player's unplayed colour (the playing cover's), a little see-through, blurring what is
  under them (`--float-bg`, `--float-blur`). The mark (the sidebar's) is only its circle,
  never on a tile; so are the web app's icons, but for the ones the platform cuts to shape (maskable, iOS, Android's
  launcher).
- **Filters** (the filter sheet, from the funnel in a floating pill; decided 2026-10-16): only the **server** a
  browser follows, with its **libraries**, is remembered, and it holds everywhere (every page, the panel, what plays);
  its cards come last in every filter sheet. On the followed server's card the libraries are switches on
  the Library, every used one (elsewhere a chip only shows a library's use), so a library is switched where its effect
  shows; the last one on stays on, so a page never filters down to nothing; and
  while one is off the funnel is lit (as the magnifier is while a search applies), the remembered filter's only trace. Every other filter lasts only for the list it was set on and is not remembered, first in the
  sheet: Playlists' Favourites only and the Library's Favourites only, Upgrades waiting (Incoming's albums that
  would upgrade one in Music), Not played yet and 4 stars and up (decided 2026-10-16, by your plays and ratings)
  (each off again on leaving the page), the panel's Hide missing tracks (off again when
  the panel shows something else), and any still to come (a tag, a genre). While any is on, the funnel is lit too.
- **Icon buttons carry no text**, on any width (decided 2026-10-16): no label that shows only when there is room. A
  long-press (touch) or right-click (desktop) on one shows its label in a small tip beside it (`lib/labels.svelte.ts`,
  one listener for the whole page: any button or link whose only content is an icon, named by its title or
  aria-label). A row's long-press (select, its menu) does not start on an icon button inside the row, and a button
  with its own long-press keeps that.
- **Over the page or beside it** (decision 0018): the player's top-right button is in every panel's top-right corner
  too (a playlist, a crate, the builder; `lib/PanelPlace.svelte`), on a window with room beside the page: beside the
  page "Over the page" (⤢), over it "Beside the page" (sidebar), always in that corner. It only changes the place:
  the player stays folded to the mini-player.
- **Controls sit next to what they change.** Sort order and size of the playlist grid are in the Playlists toolbar,
  not in Settings. Where music plays is chosen at the player (its output button), not in Settings.
- **Views of one page are tabs**: a segmented control at the start of its toolbar, remembered by the browser
  (Insights: Statistics, Related). On Related, its filters (shared tracks, genre, artists) are a filter section under the toolbar, shown or hidden by the filter button beside the views, as on Playlists.
- **Insights → Related and the panel** (decision 0023): selecting a point shows "View <playlist>" at the graph's top
  right, which views it in the shell's panel (`?view=`); while a playlist is viewed the graph's selection and the panel
  follow each other. The **Legend** button is always in the graph's bottom-right corner; it opens the legend as a sheet
  (lines, points, and the colour groups named by their most common genres).
- **Settings** is a portal of cards (decided 2026-10-16): one card per part, its name and what it is set to in a
  line, a tap opening the part in the panel (`?view=settings:<part>`, lib/SettingsPanel), as a playlist opens: beside
  the cards on a wide page (the open card bordered), over them on a phone, ✕ back to them. The parts, in this order:
  This device (in the app only), Connections (servers: address, key, status; Lidarr the same way, shown only when an
  Incoming library is set), Libraries (one row per server library: its use, Music, Incoming or Not used, and the
  folder Cratarr sees its files in, or a warning when it can't; then the names, the trash, the catalogue status and
  tag splitting), Authentication (the account Subsonic apps sign in with; decided 2026-10-16, not "Subsonic"), Import
  (plays, favourites, ratings and added dates from a server), Display (theme, progress bar). About (version, port,
  file locations) is not a part: it is a sheet opened from the info button at the page's top right, in the floating
  row (lib/AboutSheet). Each part is its own file (lib/settings/*Part) over one editor (lib/settings/editor), so what is
  changed in one part shows the Save bar in all of them and survives going from one to another.
  What a part edits is saved with the Save bar at its foot. A part opens on the settings last read (reading them
  checks every connection, which takes seconds) and takes the fresh ones unless something was edited meanwhile.
  Technical details go to About or nowhere. A use is set only there (decision 0009).

- **Shared pieces, not each page's own layout** (decided 2026-10-16): a sheet's or a setting's card is `lib/Card`
  (its title and a muted note, then what it holds); a setting's row is `lib/SettingRow` (its name at the start, its
  control at the end; where both do not fit, the control goes under the name: the row wraps, no page rule decides it);
  a choice of one as rows is `lib/ChoiceList` (round marks); a few choices in a row `lib/Seg` (its words never break:
  the row takes the control under the name instead); on/off `lib/Switch`. View, More music, Settings' Display and
  Libraries are made of them.
- **Phone check** (`npm run check:phone`, decided 2026-10-16): every page, every part of the settings and an album in
  the panel at 390 px must not scroll sideways (the page, or any box that scrolls up and down); run it after a layout
  change.

## Rows

Every list of things is built from one row (decision 0027, `lib/Row.svelte`), so a playlist, a crate, a file, a track
or a sync target looks and behaves as one kind of thing wherever it is listed.

- **One layout, three sizes.** Left to right: an optional lead (a track's cover, a checkbox), the title (two lines at
  most) with its marks after it and an optional muted line under it, its count and flags at the end of the text, and
  the action on the right. **Medium** on the pages (Playlists, Crates) and **small** in the panel's Export tab are
  Finamp's add-to-playlist tile (tinted in the accent, a divider before the action); **large** in the panel's Tracks,
  Info, Repair and Sync tabs, the crate builder's preview and Health's files is Finamp's list tile (plain, 64 px, a
  60 px cover or a control in front). A track is a large row with its cover (`lib/TrackCover.svelte`).
- **States are the row's:** viewed in the panel or ticked: a border in the accent; holding what plays: a stronger tint;
  not usable now (a missing file): its contents dimmed (the row stays opaque).
- **What is listed has its own row on top of it,** the same wherever it shows: `lib/PlaylistItem.svelte` (the name, the
  pin and own-account marks, the count, a favourite's heart over the problems' dot), `lib/FileItem.svelte` (the file,
  and under it its album folder and what is wrong, or "→ the file a repair points it at"). Where it is decides only
  the size, the second line and the action.
- **A large row's controls may go under its text** when the panel is narrow (Sync's settings, Repair's buttons): a
  row never scrolls sideways.
- **The Library** (decision 0034, `Library.svelte` over `lib/AlbumShelf.svelte`, decision 0028): every album of the
  followed server's Music and Incoming libraries as one list, the libraries switched in the filter; the order (Artist,
  Title, Year, Added; Added newest first at first; and by how you used them, decided 2026-10-16: Plays, Last played,
  Rating, most first when chosen, the value on a tile's corner pill), Rows or Covers, and Size, in columns as the playlist chips are.
  Rows are extra large rows (the cover, the year after the title, the artist, tracks, format and when it was added,
  and an Incoming album's lines: its conversion and how it relates to Music; an album of a library other than Music
  has a dot in its library's colour after its title, while more than one library shows); its menu is the album's one
  menu (above); Covers are tiles (the dot before the title). A row's cover
  plays the album (the rest of the row views it); a row ends in ⋮ (its menu): **wherever a cover plays, its play
  icon is in the cover's lower right corner**, as on a tile (decided 2026-10-16). **Select** as on every list: the
  header's icon or a long-press ticks albums (a row lighter edge to edge, a tile bordered, the tick at a row's end or
  over a tile's corner), and the select bar plays, puts next, queues, favours (in the app downloads) the ticked ones,
  one album after another, or opens the menu of one (a tile's menu is this way). Layout and
  size are one choice for every album list, remembered by the browser.
- **Tiles** (`lib/Tile.svelte`): Row's sibling for things shown by their picture: the square picture on top, as wide
  as the column the page's Size makes, the title and muted lines under it (an album: its artist only, and the page's
  own lines), the action (Play) over the picture's lower right corner, and over its upper left one a pill with what
  the list is ordered by when the lines do not say it already (an album list by Year or Added; decided 2026-10-16).
  The album lists' View also has **Dates**: Relative ("3 days ago") or Absolute ("14 Oct"), remembered. Its states are the row's: a border when viewed, a tint when it holds what plays.
- **A list's sections** (`lib/Section.svelte`): a heading with its count that folds the list, and while open one line
  on what the list is. A selection's Tracks (a section per playlist), the Repair tab's kinds of repair, the Info tab
  for several playlists, Health's kinds of problem (folded at first).

## Gestures

- **Tap does the page's main action** on an item: on Playlists add or remove the playing track (the circle; the
  name views the playlist, in Select ticks it); on the Library view the album in the panel
  (its cover's play icon plays it); on Insights' graph highlight the playlist and its closest playlists.
- **Long-press (right-click with a mouse) opens the item's menu** when the item has more than one other action. Every
  menu is a **bottom sheet like Finamp's track menu** (decision 0021, `lib/MenuSheet.svelte` + `lib/MenuEntry.svelte`),
  the same component on every page: at most 500 px wide, rising from the bottom, a drag handle; the item on a card at
  its top (its image: a cover, or a playlist's mosaic of its first four albums; its title; small chips; for a track
  the heart); its playback actions as a row of large icons with small labels; then the other entries as rows with an
  icon in the accent colour. The sheet takes its look from the item's image (blurred behind it, its accent).
  For a playlist: Play, Shuffle, Play next, Add to queue; View, Info, Export…, Repair… and Sync… (the panel's views),
  Spoken as…, Rename, Delete. For the playing track
  (the player's menu button, or a right-click on its cover): Play next, Add to queue; Add to playlist, the favourite,
  View album, New playlist from the queue. In the app (not a browser) a track's, an album's and a viewed playlist's
  menus also have **Download** (an arrow down to a line): its tracks onto the phone, shown on Downloads. **View** (the square with an arrow out of its corner) is every menu's entry
  that shows the item in the panel: a playlist, an album, a track's album (View album). A tap on the backdrop, Esc, Back or dragging the sheet down closes it. With a single
  other action, the long-press does that action directly.
- Inside the sheet: destructive entries last and in red. Spoken as…, Rename, the playlists for a track and
  the delete confirmation are steps inside the same sheet (it grows or shrinks to fit, a back arrow returns), not a
  separate dialog.
- A long-press never also triggers the tap, and suppresses the browser's own menu and text selection.
- **In the panel's track list a long-press starts select mode** (Symfonium's), with that track ticked; the track's menu
  is its ⋮. Several playlists' sections, which have no select mode, keep the long-press menu.

## Words

- Plain words, sentence case ("Add to queue", not "Add To Queue"). No internal terms on screen: not "source",
  "placement", "root", "entry", "line", "sync".
- **Vocabulary:**
  - *track*, *album*, *playlist*.
  - *Music* and *Incoming*: the two library uses, named exactly as in Settings.
  - *missing track*: a playlist line whose file is gone.
  - *player*: an app on a device, written "Finamp on Lenovo Tab".
  - *server*: Jellyfin and the like (not "source").
- **Counts say what they count:** "in 3 playlists", "12 tracks", "92 missing tracks".
- **Buttons are verbs** saying what happens: "Fix 92 missing tracks", "Delete playlist", not "OK" or "Submit".
- **Help text** is one sentence under the thing it explains, only where the label is not enough.

## Feedback

- **What an action did, or why it failed, is a toast** (`lib/toast.svelte.ts`): one at a time, floating above the
  mini-player, or the bottom edge without it (beside the sidebar from 1024 px), never moving the page. Neutral for done ("Deleted Chill."), amber for
  warnings, red for errors. Done and warnings go after a few seconds; errors stay until dismissed, replaced, or the
  page changes (not when the panel's view comes or goes). Errors say what failed in plain words, never a bare status code.
- **Undo is in the toast right after the action, and only there** (decision 0026, `lib/undo.svelte.ts`): every change
  to playlists, crates or Incoming says what it did with Undo (`withUndo`); Undo says "Undone." with Redo; Redo says
  the action's words again, with Undo. A refused undo says why ("It changed since; nothing was undone."). What one tap
  undoes by tapping again (a track into a playlist, a heart, a pin) has no Undo.
- **A page's own state stays in place** (`lib/Notice.svelte`): the page could not load *and has nothing to show*, a
  field's question (the Keep year), empty and loading states. Nothing temporary sits in the page: a reload or update
  that fails while the page shows its content is a toast (once, gone when the server answers again), and so is the
  answer to a check (Check connection: connected or why not).
- **Waiting:** a thin bar along the top while something the user asked for loads (`lib/ProgressBar.svelte`): only
  after 300 ms, so quick answers show nothing. Background polling and actions shown at once (the player, playlist
  taps, the favourite) never show it (`quietly` in `api.ts`). The button that was tapped shows a spinner in place of
  its label (`aria-busy` with `disabled`) until the answer. No full-page spinners.
- **Destructive actions** ask in place, naming what goes ("Delete Chill?"); nothing destructive happens on one tap.
- **Loading:** "Loading…" in the content area; never an empty page. **Empty:** say why and what to do ("Nothing
  incoming." / "No Music library is set. Choose one in Settings → Libraries.").

## Checking

- Every interface change is looked at on a phone width (390 px) and a desktop width before release, with real
  data, and checked against this file.
