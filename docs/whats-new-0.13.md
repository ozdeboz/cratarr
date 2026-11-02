# What's new in 0.13

Since 0.12.4; release/0.12's fixes after it are in it too. 0.13 is what preview.3 holds (the owner, 2026-11-01): modes per server, scrobbling from the device and each
user's own music moved to 0.15 and 0.16 ([plans](plans.md)). The notes for each part are in [decisions](decisions).

## Crates

- **One crate, two views** ([0088](decisions/0088-one-crate-two-views.md)). Every crate has a Sentence and a
  Conditions view of the same conditions; a change in one shows in the other. Sounds like blends its artists ("and")
  or takes any one of them ("or"); a genre, style, mood or tag takes any or all of its values.
- **Shown and built in the panel.** Crates' + opens the builder empty; a crate of your own heads its tracks with its
  sentence, and a change there opens its edit mode. Crates searches its crates as every page does.
- **The sentence reads more.** "but not", "without", "except", "excluding" and "no" leave a term out ("jazz and blues
  but not techno"); "before", "after", "since" and "until" a year or a decade make an open span, and "seventies",
  "70s-80s" and "1970s-80s" are decades. Two terms of one kind joined by "or" or "and" make a crate ("jazz or
  blues"), as does one kind with something left out.
- **One crate card atop every search.** Where the words read as a crate, global search and every page's own search
  lead with it: its covers, its sentence (each term a tap), its count, Grab this crate and Play. Nothing shows while
  it loads, and nothing where the words make no crate.
- **Play spreads records:** never two tracks of one record back to back, with the same spread on the server and the
  page. The built-in crates say in a few words what they are ("sounds like your week").
- **A record in a crate opens in a sheet** of its crate tracks: Play and Shuffle, each track with its snippet. In a
  crate's or playlist's flow of covers, a tap on the right half brings the next record forward, on the left the one
  before, and on the front cover opens its tracks.
- **A crate's head:** Edit left of the heart for a crate of your own; a tap on its count switches records and tracks;
  the crates that can't be edited (Home's, Sounds like) have the Filter.

## Search and moving about

- **A page's search shows its list as the table**, on a phone too; emptied, the list comes back where it was. Where
  the words read as a crate, the page's list matches by its terms.
- **An empty search says so from the magnifier:** "No records for [words ✕]", the ✕ clearing them, on Records,
  Composers, Playlists, Crates, Discover, Health and Settings.
- **Global search follows you one page only:** the next page opens on its own search with the same words.
- **One loading rule:** a list waiting for its answer keeps what it showed, the loader taking its place at 300 ms; a
  page swap shows its sign at 300 ms too.
- **Scrolled to what a link leads to.** A track's Go to album, a track found by global search, Home's track cards and
  the player's album open the album at that track, marked. Beside the panel, the page's records, crates and
  playlists bring what it opened into sight. Nothing a page scrolls to ends up under the bar, the mini-player, a
  sticky action bar or the glass.
- **The glass moves as one.** The floating controls and the bar of pages change shape in one smooth step: the
  search opening, More, a page's buttons coming and going, the title making way. Nothing in them is laid out again
  while they move, so they keep moving while a page is busy drawing.
- **Typing on a phone:** the bar and the glass at the top stay where they are; along the bottom only the glass being
  typed in stands over the keyboard, the rest making way. With the search open, the Library's picker makes way for the
  field rather than cutting it short.
- **The loading sign sits still** while the page under it is scrolled.
- **Back undoes a filter.** A pick beside back, or what was changed while the Filter sheet was open, is one step: Back
  shows the list as it was (Library, Playlists, Downloads).
- **Breadcrumbs:** a tap on a level opens its siblings; a long-press switches between the short trail and the whole
  one. Settings, Health and Insights lead with their group.
- **Back and the pill are one glass each,** their buttons on it; what goes beside them stretches the glass. Menus
  from the glass have its look and stay inside the window.
- **Asked in a sheet:** what needs a yes (Reset, Remove, Replace) asks in a sheet from the bottom.
- **An error says so,** with Try again, never shown as an empty list or a wait without end.

## The Library's types

- **Three layers** ([0081](decisions/0081-release-types.md), amended): one release type (Album, EP, Single,
  Broadcast, Other), any number of extra types (Compilation, Live, Soundtrack…), and Classical. A record counts under
  every type it has. Incomplete is no type now: a record missing tracks is its whole release's type, and its panel says
  "7 of 12 tracks", a tap opening Health.
- **The picker beside back** lists every type the Library has, most records first, each with its count. One type
  reads as its name ("Albums", "Live"); more read "Selected types".
- **The Filter sheet's Type row** says what it asks: "or" within a layer, "and" across ("Album or EP and Live").
- **Records from other servers** are typed by the same rule. The library is built again once on the first start.

## Records

- **Edit** ([0089](decisions/0089-facts-set-by-hand.md); administrators'): a record's type, year, secondary types,
  genres and Classical set by hand, for one record or a selection, each with a Reset.
- **Classical says why** a record is or is not; a compilation or soundtrack goes by most of its tracks, not one; works
  show on classical records only; names in Latin letters.
- **Playlists** order by Length and Last played (their columns too), with a Missing column. The records' Format column
  is named Quality, as its order.

## Health

- **Laid out as Settings:** problems grouped by kind as rows; a problem opens in one panel, its items opening in
  place; Work through takes them one at a time.
- **Disc numbers:** albums whose track numbers start again without disc tags, or whose release has more discs than the
  files say, with Renumber.
- **Missing tracks, many at once:** tick albums, Cratarr searches Soulseek for their missing tracks in turn, and one
  review list shows what it found; nothing goes in without your tick, and each track has its Undo.

## Changes to files

- **One way to change files** ([0092](decisions/0092-one-way-to-change-files.md)). Every change Cratarr makes to the
  library's files is reviewed first, written one album at a time, recorded with one Undo, and shown the same way.
- **Write types and years into files** (Settings → Libraries; off at the start): Health then lists the records whose
  files differ from Cratarr's types, year and Classical. **Review** shows each record's fields as the files have them
  and as Cratarr has them, and where from; tick what to write, **Write**.
- **The record says it** under its title: its files differ (Review), are waiting, are being written, were written
  (Undo), or a file changed since (Review).
- **Changes to files** (Health → Files) lists every run: the one writing and those waiting, then by day with Undo, and
  what was set aside and why. Undo puts back only what still holds what was written.
- **A file's other tags stay as they were:** writing a tag changes only the tags it writes.

## Connections

- **Cratarr as an MCP server** ([0090](decisions/0090-mcp-server.md)): an assistant reads your library through
  `/mcp`, with a read-only key from Settings → Connections → MCP (search, records with their type and why, classical
  composers and works, Health's findings, your crates).

## The player

- **The wind window** keeps the crosshair in its middle every frame, and at a wall it is never wider than centring
  allows.
- **The tape deck** keeps its narrow-panel size at any width, and its row keeps its height.
- **A track's menu** has Exclude this track everywhere as a switch beside the artist's.

## Settings

- **Saved as they change:** each sheet's OK, switch or choice saves, no Unsaved changes bar. A connection's address,
  user and secrets are one sheet with one Save; refused, the settings stand as they were and why is said.
- **One Reset sheet:** a part's Reset (on a phone in the pill) puts back all of it, a section's Reset its own, each
  listing what changes.
- **Libraries of Cratarr's own have a name,** their folder's until another is given. Checks, Keep and Replace, and
  Health look across every library; Incoming's tracks go in playlists and exports.
