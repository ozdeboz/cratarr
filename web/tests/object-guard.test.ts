// OBJ's guard (PLAN-0.18 OBJ, the owner 2026-11-20: "the spine"; OBJECT-MAP-2 §3; LANES v2 §3.4). Every object (a record, a track,
// an artist…) gets one card, row, chip, sheet, name and picture: its parts. A page that draws one of them its own way, instead of
// through the object's part, is a site here, counted per object and part, and every count is pinned:
// - a count above its pin fails: a new own way (draw it through the object's part, or build the part);
// - a count below its pin fails too, until the pin is lowered with it (a part landed, or DEL took an old way), so the pins stay true.
// The player's objects (Play On target, queue, Ask) are 0.19's (the owner): `held`, so 0.18 only keeps them from growing.
//
// The rules, simple on purpose:
// 1. The page sources: web/src's .svelte and .ts files, lib/generated (the server's types) left out.
// 2. An object's marks: each a regular expression for one shape of drawing one of its parts, with the surface it stands for. A
//    shape is a component or function of its own for the part, markup or a template written in place, or a shared piece (Row,
//    Tile, Cover, a MenuSheet's head, a chip) composed with the object's own fields. A mark without `in` looks everywhere, so its
//    shape copied anywhere counts too; one with `in` looks in that file only, for a surface only its place tells apart.
// 3. A site is one match of a mark. The object's `one` for a part is the file that is, or will be, that part: it holds no sites.
//    Going through a part (mounting the one card, calling the one formatter) is never a site.
// 4. Each count is its sites, pinned in `pin`; `map` keeps OBJECT-MAP-2 §3's count beside it. They differ where the map counts
//    the server's and Android's ways (out of this scan), grouped surfaces a rule counts one by one, or missed a surface; the pin
//    is what the scan finds (P/PAGE-REPORT.md lists each difference). No site counts twice for one object; a surface the map
//    lists under two objects counts for both.
// When a part lands: name its file as the object's `one` for it, move the surfaces onto it, and lower the pin to what is left. A
// site a mark finds wrongly (not that object's part): tighten the mark, never raise the pin for it.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';

type Part = 'card' | 'row' | 'chip' | 'sheet' | 'name' | 'picture';
const PARTS: Part[] = ['card', 'row', 'chip', 'sheet', 'name', 'picture'];
type Counts = Partial<Record<Part, number>>;
type Mark = { part: Part; re: RegExp; in?: string; why: string };
type Obj = {
  /** OBJECT-MAP-2 §3's count, for the record. */
  map: Counts;
  /** What the scan finds, pinned. */
  pin: Counts;
  /** 0.19's (the player's objects): held from growing in 0.18. */
  held?: true;
  /** The file that is (or will be) the object's part, by part: it holds no sites. */
  one?: Partial<Record<Part, string>>;
  marks: Mark[];
};

const OBJECTS: Record<string, Obj> = {
  // Chip and name: no one yet (OBJ's record parts); the type's words (recordWord, goToName, viewName) are taken as the one.
  Record: {
    map: { card: 5, row: 8, chip: 5, sheet: 6, name: 9, picture: 3 },
    pin: { card: 6, row: 13, chip: 8, sheet: 8, name: 31, picture: 3 },
    one: { card: 'lib/AlbumCard.svelte', row: 'lib/ReleaseRow.svelte', sheet: 'lib/sheet/Record.svelte', picture: 'lib/api.ts' },
    marks: [
      { part: 'card', re: /<Tile\b(?:(?!<\/Tile>)[^]){0,900}?albumSubject\(/, why: "a Tile drawn with a record (the Library's grid tile)" },
      { part: 'card', re: /\{#snippet \w+\(\w+: DiscoverAlbum\)\}/, why: "Discover's card of its own" },
      { part: 'card', re: /\{#if b\.kind === 'album'\}/, in: 'Home.svelte', why: 'Home\'s Jump back in tile ("Album · <by>")' },
      { part: 'card', re: /<CoverFlow\b[^>]{0,40}?covers=\{flowShown\}/, in: 'lib/PlaylistView.svelte', why: "PV's CoverFlow of a crate's records" },
      { part: 'card', re: /<(?:CoverFlow|FlowStage)\b[^>]{0,80}?covers=\{\w+\.map\(\(\w+\) => albumCoverUrl\(/, why: "a coverflow of records, the front one named under it (Home's hero, the Library's Flow)" },
      { part: 'card', re: /<div class="pair">/, in: 'lib/HealthIncoming.svelte', why: 'HealthIncoming: the two copies side by side (also an Arrival card)' },
      { part: 'row', re: /<Row\b(?:(?!<\/Row>)[^]){0,700}?(?:<CoverStatus\b|albumCoverUrl\()/, why: "a Row drawn with a record's cover or its request mark (search, Find, LabelBody, DiscoveryList, WorkSheet, excluded albums, browser downloads)" },
      { part: 'row', re: /<Row\b[^>]{0,40}?title=\{\w+\.title \|\| 'Unknown album'\}/, why: "a crate's record as a Row (CrateResults, PV's recordRow)" },
      { part: 'row', re: /<Row\b[^>]{0,40}?title=\{\w+\.album\}/, why: "a Row titled by a record's album field (DiscoveryList's albums left out)" },
      { part: 'row', re: /<Row\b[^>]{0,120}?title=\{a\.title\}/, in: 'lib/AlbumShelf.svelte', why: "AlbumShelf's table and list rows" },
      { part: 'chip', re: /goToName\(/, why: 'a "Go to album" entry written in place (TrackMenu, SelectionMenu, HealthPanel)' },
      { part: 'chip', re: /<button class="album-name"/, why: "NowPlaying's album name as a link" },
      { part: 'chip', re: /chips=\{\[\w+\.artist, \w+\.album\]/, why: "TrackMenu's head: the record as a chip" },
      { part: 'chip', re: /\{#if l\.kind === 'have'\}/, in: 'lib/PlaylistView.svelte', why: 'PV\'s not-had line naming the record the library has ("In your library")' },
      { part: 'chip', re: /label="View" onclick=\{[^}]*albumSubject\(/, why: 'a bare "View" entry opening a record (RequestsPanel\'s request and Soulseek sheets)' },
      { part: 'sheet', re: /<MenuSheet\b/, in: 'lib/BandcampAlbumMenu.svelte', why: 'BandcampAlbumMenu' },
      { part: 'sheet', re: /<MenuSheet\b/, in: 'lib/RecordSheet.svelte', why: "RecordSheet (a crate's record)" },
      { part: 'sheet', re: /<MenuSheet\b/, in: 'lib/RecordProblemSheet.svelte', why: 'RecordProblemSheet (also an Arrival sheet)' },
      { part: 'sheet', re: /<MenuSheet\b/, in: 'Discover.svelte', why: "Discover's hand-built sheet" },
      { part: 'sheet', re: /<MenuSheet\b/, in: 'LocalAlbums.svelte', why: "LocalAlbums' hand-built album sheet (local and server mode)" },
      { part: 'sheet', re: /<SelectionMenu\b[^>]{0,40}?title=\{\w+\.title \|\| 'Unknown album'\}/, why: "SelectionMenu as a crate record's menu" },
      { part: 'sheet', re: /<SelectionMenu\b[^>]{0,200}?recordsWord\(/, why: "SelectionMenu with SeveralAlbums, the several records' sheet" },
      { part: 'name', re: /\$\{(?:\w+\.)?artist\} – \$\{(?:\w+\.)?(?:title|album)\}|\[(?:\w+\.)?artist, (?:\w+\.)?(?:title|album)\]\.filter\(Boolean\)\.join\(' – '\)/, why: '"artist – title" written out in place' },
      { part: 'name', re: /\{(\w+)\.artist\}\{\1\.year \? ` · \$\{\1\.year\}` : ''\}/, why: '"artist · year" written out as a second line' },
      { part: 'name', re: /(?<!const head = )\[(?:\w+\.)+artist(?: \?\? '[^']*')?, (?:\.\.\.\()?(?:\w+\.)+year\b/, why: "a record's artist and year as a list of lines or chips" },
      { part: 'name', re: /\w+\.artist\b[^\n]{0,100}?holdsLine\(/, why: "a crate record's line: its artist and how much of it the crate holds" },
      { part: 'name', re: /\(\w+: ListedAlbum\) => \(\w+\.performers\?\.length/, why: "AlbumShelf's byline (performers, else the artist)" },
      { part: 'name', re: /\['Album', \w+\.by\]/, why: 'Home\'s "Album · <by>" whatever the type' },
      { part: 'name', re: /export (?:const|function) (?:mainArtist|creditsOf)\b/, why: "credit splitters of their own (mainArtist, creditsOf; also the Artist's)" },
      { part: 'picture', re: /coverUrl\(\{[^}]*\bid: first\.id\b/, why: "a record's picture as its first track's cover (RecordSheet, CrateResults)" },
      { part: 'picture', re: /\{@render cover\(first, source\)\}/, in: 'lib/PlaylistView.svelte', why: "PV's crate record row: its first track's cover" },
    ],
  },
  // Left unmarked as the one to be: the row, PV's trackRow; the card, QueueSheet's playing card; the name, trackRow's line.
  Track: {
    map: { card: 1, row: 9, sheet: 2, name: 12, picture: 4 },
    pin: { card: 2, row: 12, sheet: 2, name: 21, picture: 9 },
    one: { sheet: 'lib/TrackMenu.svelte', picture: 'lib/api.ts' },
    marks: [
      { part: 'card', re: /<span class="title">\{t\.name[^}]*\}<\/span>/, why: "Home's ul.back track tile (Blast from the past, Favourite tracks)" },
      { part: 'row', re: /<Row\b[^>]*?title=\{(?:x\.)?r\.name \?\? (?:x\.)?r\.file\}\s+sub=/, why: 'a Row titled name ?? file with its own sub line (PV edit and removed rows, SnippetRows, ExcludedList, NotForSheet)' },
      { part: 'row', re: /sub=\{albumRowSub\(/, why: "PV's albumRow" },
      { part: 'row', re: /<button\s+class="row"/, in: 'lib/QueueSheet.svelte', why: "QueueSheet's own button.row (previous, upcoming, asked)" },
      { part: 'row', re: /<button class="track"[^]{0,200}?coverUrl\(/, why: "SuggestedTracks' own button.track row" },
      { part: 'row', re: /<Row\s+size="medium"\s+title=\{label\}/, in: 'lib/WorkSheet.svelte', why: "WorkSheet's movement row (a work's track; the map's WorkSheet:103)" },
      { part: 'row', re: /<Row size="large" title=\{it\.kind === 'artist' \? artistLabel\(/, in: 'lib/GlobalResults.svelte', why: "GlobalResults' one Row for every kind (a track result)" },
      { part: 'sheet', re: /<MenuSheet\b[^>]*?title=\{(?:track|t)\.name\}/, why: "HeartSheet's own track sheet head" },
      { part: 'sheet', re: /<MenuSheet\b/, in: 'lib/SelectionMenu.svelte', why: "SelectionMenu's own sheet of ticked tracks" },
      { part: 'name', re: /\.name \|\| '…'/, why: "the title as '…' when empty (QueueSheet's previous row, port's queue and now)" },
      { part: 'name', re: /\.title \?\? \w+\.file\}/, why: 'the title as title ?? file (HealthRenumber)' },
      { part: 'name', re: /sub=\{(?:x\.)?r\.artist \?\? ''\}/, why: "a row's second line as the artist alone (SnippetRows, ExcludedList, NotForSheet, PV removed)" },
      { part: 'name', re: /sub=\{r\.missing \? 'Missing file'/, why: "PV edit row's second line" },
      { part: 'name', re: /export function albumRowSub\(/, why: 'albumRowSub (recordNotHad.ts)' },
      { part: 'name', re: /\{#snippet byline\(t\b/, why: "QueueSheet's byline snippet" },
      { part: 'name', re: /\[\w+\.artist, \w+\.album\]\.filter\(Boolean\)\.join\(' · '\)/, why: 'artist · album joined in place (SuggestedTracks, Home favourites, serverMode search, the guest page)' },
      { part: 'name', re: /chips=\{\[t\.artist, t\.album\]\.filter\(/, why: "TrackMenu's chips" },
      { part: 'name', re: /\{t\.artist\} · \{ago\(/, why: "Home's Blast from the past second line" },
      { part: 'name', re: /\{np\.artist \?\? ''\}/, why: "the playing track's artist line (MiniPlayer, QueueSheet's playing card)" },
      { part: 'name', re: /\{#if np\.artist && np\.album\}<span class="dot">·<\/span>/, why: "NowPlaying's artist · album line" },
      { part: 'name', re: /\[artist, name\]\.filter\(Boolean\)\.join\(' – '\)/, why: "TrackMenu's Share: artist – name" },
      { part: 'picture', re: /\[library, album\]/, why: "coverFor's candidate order, and accentFor's copy of it" },
      { part: 'picture', re: /(?<!const album = t\.album_id \? )albumCoverUrl\((?:(?:t|track)\.source|'cratarr'), (?:t|track)\.album_id\)/, why: "a track's picture taken from its album's cover" },
    ],
  },
  // Left unmarked as the one to be: the row, GlobalResults' artist row; the chip, PV's record chips (albumChips). The
  // name's and the picture's one is the server's (the credit split, the photo field), so every page way counts.
  Artist: {
    map: { row: 6, chip: 8, sheet: 1, name: 6, picture: 5 },
    pin: { row: 6, chip: 9, sheet: 1, name: 5, picture: 5 },
    one: { sheet: 'lib/ArtistMenu.svelte', card: 'lib/ArtistsToExplore.svelte' },
    marks: [
      { part: 'row', re: /<Row\b[^>]*?title=\{artistLabel\(/, why: "Find's Beyond artist row" },
      { part: 'row', re: /<Row\b[^>]*?onclick=\{\(\) => goToArtist\(/, why: "SpotifyLibrary's artist row" },
      { part: 'row', re: /<Item name=\{a\.name\} line=\{a\.at \?/, why: "TastePart's excluded-everywhere artist Item" },
      { part: 'row', re: /<ReleaseRow\s+cover=\{keptPicture\(i\)\}/, why: "DownloadsPanel's synced row (an artist kept), also in select mode" },
      { part: 'row', re: /<li class="plain">\s*<span class="rank">/, why: "Insights' top artists" },
      { part: 'chip', re: /<button class="chip" onclick=\{\(\) => viewOver\(artistSubject\(/, why: "PV's All-tracks chip" },
      { part: 'chip', re: /<button class="chip"[^>]*?onclick=\{\(\) => open\(a\)\}/, why: "RelatedArtists' chips" },
      { part: 'chip', re: /<button class="chip" class:on=\{i === open\}/, why: "ArtistsToExplore's chips" },
      { part: 'chip', re: /Typical artists<\/span>\{#each/, why: "PV's Typical artists" },
      { part: 'chip', re: /aria-label="Take \{[\w.]+\} back into this crate"/, why: "Not in this crate: PV's line, CrateEditor's copy, CrateSentence's 'but not by'" },
      { part: 'chip', re: /<span class="chip" class:past=/, why: "ArtistBody's Real name / Members / In / Also as spans" },
      { part: 'chip', re: /<button class="artist"[^]{0,80}?goToArtist\(/, why: "NowPlaying's whole artist line as one button" },
      { part: 'sheet', re: /<ArtistMenu\b(?:(?!\bid=)[^>])*\/>/, why: 'the artist sheet mounted without their id (ArtistRowMenu, by name)' },
      { part: 'name', re: /export function albumChips\(/, why: 'albumChips (artistIds.ts)' },
      { part: 'name', re: /export function artistLabel\(/, why: 'artistLabel, the namesake number (artistIds.ts)' },
      { part: 'name', re: /(?<=[(=,]\s*)\/(?:\\.|[^/\\\n])*?feat(?:\\.|[^/\\\n])*\/[a-z]*/, why: "a credit split written in the page (creditsOf's SPLIT, artistCredits, mainArtist)" },
      { part: 'picture', re: /artistPhotoUrl\(/, why: "a page's own pick of the artist's photo (PV heroArt's, PV's chips', ArtistsToExplore's two, RelatedArtists')" },
    ],
  },
  // The name's one is standing() (requestState.ts, which also holds cardStanding, so no file is named).
  Request: {
    map: { row: 1, chip: 6, sheet: 4, name: 5, picture: 1 },
    pin: { chip: 4, sheet: 3, name: 4, picture: 1 },
    one: { card: 'Discover.svelte', row: 'lib/RequestsPanel.svelte', chip: 'lib/CoverStatus.svelte', sheet: 'lib/RequestsPanel.svelte' },
    marks: [
      { part: 'row', re: /<(?:ReleaseRow|Row|Item)\b[^\n]{0,200}?\b(?:line|sub)=\{[^}\n]{0,40}\b(?:standing|cardStanding)\(/, why: "a row whose line is a request's standing, written in place (none today beside RequestsPanel's)" },
      { part: 'chip', re: /\)\s*:\s*\{\s*state:\s*'wanted'\s*\|\s*'downloading'/, why: "the page's own cover-mark rule (markOfState; the server's mark() is the other)" },
      { part: 'chip', re: /<button\b[^\n]{0,100}?hrefOf\('requests'\)/, why: "the not-had page's request pill (it opens the Requests page, not the request)" },
      { part: 'chip', re: /'requested' \? 'Requested'/, why: 'Spotify\'s own "Requested" for a requested track or album' },
      { part: 'sheet', re: /<MenuGroup\b[^>]*>(?=(?:(?!<\/MenuGroup>)[^]){0,400}?\bRequested\b)/, why: 'a menu group offering Want/Requested of its own (Discover, DiscoverEntries, AlbumMenu)' },
      { part: 'sheet', re: /\brequested:\s*\{\s*icon:/, why: "the not-had header's Want/Requested action (PV)" },
      { part: 'name', re: /export function cardStanding\(/, why: "cardStanding(), a card's own words beside standing()" },
      { part: 'name', re: /Record<StoryLidarr\['step'\], string>/, why: "requestStory's Lidarr step words" },
      { part: 'name', re: /aria-label=\{mark\.state === 'wanted' \? 'Requested'/, why: "CoverStatus's own words (Requested, Downloading N%)" },
      { part: 'name', re: /'Requested' : 'Approved'/, why: "Discover's own words from the raw status (Requested, Approved)" },
      { part: 'picture', re: /coverartarchive\.org\/release-group\//, why: "a release group's cover URL built on the page (the Lidarr list's coverOf)" },
    ],
  },
  // Left unmarked as the one to be: the chip, PV's word chips; the name, crateNameKnown.
  Crate: {
    map: { card: 3, chip: 3, name: 3, picture: 5 },
    pin: { card: 3, chip: 3, name: 3, picture: 6 },
    one: { card: 'lib/CrateCard.svelte', sheet: 'lib/CrateMenu.svelte', picture: 'lib/crates.svelte.ts' },
    marks: [
      { part: 'card', re: /<CoverCard\b/, why: "a crate on the week's CoverCard (Home's Sounds like your week, SearchCrateCard)" },
      { part: 'card', re: /class:mosaic=\{it\.art\.length === 4\}/, why: "SimilarLists' card (a crate's)" },
      { part: 'chip', re: /<button[^>]*?onclick=\{\(\) => viewGenre\(/, why: "a genre chip written in place (PV's genres, AlbumAbout, Home's week pills)" },
      { part: 'name', re: /HOME_CRATES: Record<string, string> = \{/, why: "Home crates' names, the page's copy of the server's (crates.svelte.ts)" },
      { part: 'name', re: /export function nameOf\(chips/, why: "the builder's nameOf (quickCrate.ts)" },
      { part: 'name', re: /`Sounds like \$\{/, why: "'Sounds like …' made in the page (the server's words)" },
      { part: 'picture', re: /const rowCovers = \$derived/, why: "PV's rowCovers" },
      { part: 'picture', re: /const flowCovers = \$derived/, why: "PV's flowCovers" },
      { part: 'picture', re: /\.map\(\(c\) => coverUrl\(\{ source: [\w.!]+, id: c\.track, album_id: c\.album_id \}\)\)/, why: "first tracks' covers (SearchCrateCard, Home's week)" },
      { part: 'picture', re: /i\.kind === 'crate' \? crateImageUrl\(/, why: "DownloadsPanel's crateImageUrl only" },
      { part: 'picture', re: /<TrackCover src=\{first\?\.id && !first\.missing \? coverUrl\(/, why: "CrateResults' per record (its first track's cover)" },
    ],
  },
  // The Incoming dot is every library's dot (not told apart); HealthPanel's intake row goes through the record's row.
  Arrival: {
    map: { card: 1, row: 1, chip: 1, sheet: 2, name: 5 },
    pin: { card: 0, row: 1, chip: 1, sheet: 2, name: 2 },
    one: { card: 'lib/IntakeCard.svelte', sheet: 'lib/sheet/Record.svelte' },
    marks: [
      { part: 'card', re: /<div class="pair">/, in: 'lib/HealthIncoming.svelte', why: 'HealthIncoming: the two copies side by side' },
      { part: 'row', re: /\{#snippet extra\(s: ShelfSource, a: ListedAlbum\)\}/, in: 'Library.svelte', why: "the Library tile's Incoming lines (conversion, verdict)" },
      { part: 'chip', re: /\{#snippet \w+\(\w+: CopyLine\)\}/, why: "a copy's line with its library's dot (PV's copy lines)" },
      { part: 'sheet', re: /<MenuSheet\b/, in: 'lib/ReviewSheet.svelte', why: 'ReviewSheet ("Before adding")' },
      { part: 'sheet', re: /<MenuSheet\b/, in: 'lib/RecordProblemSheet.svelte', why: 'RecordProblemSheet (also a Record sheet)' },
      { part: 'name', re: /name="This copy" line=/, why: 'IntakeCard\'s "This copy" line' },
      { part: 'name', re: /const libraryName = /, in: 'lib/HealthIncoming.svelte', why: "HealthIncoming's side named by its library" },
      { part: 'name', re: /'Upgrade' : `In \$\{/, why: 'the Library tile\'s verdict badge ("Upgrade", "In <library>")' },
    ],
  },
  // Left unmarked as the one to be: the card, SimilarLists' card. The picture's one is the server's pick.
  Playlist: {
    map: { card: 1, row: 1, sheet: 1, picture: 2 },
    pin: { card: 1, row: 2, sheet: 1, picture: 3 },
    one: { row: 'lib/PlaylistItem.svelte', sheet: 'lib/PlaylistMenu.svelte' },
    marks: [
      { part: 'card', re: /<Icon name=\{b\.kind === 'queue' \? [^}]*: 'playlists'\}/, why: "Home's Jump back in tile: a playlist's icon, no cover" },
      { part: 'row', re: /<ReleaseRow\s+cover=\{keptPicture\(i\)\}/, why: "DownloadsPanel's synced row (a playlist kept), also in select mode" },
      { part: 'sheet', re: /<MenuSheet label="Playlist"/, why: "PV's server-mode playlist sheet outside the table" },
      { part: 'picture', re: /playlistCovers\(\w+(?:\[0\])?\)\.then\(\s*\(list\) =>/, why: "a playlist's covers made its picture in place (PV, PlaylistMenu, SimilarLists)" },
    ],
  },
  // The name's one is the server's word_of, so the page's itemLine is a site.
  'Soulseek item': {
    map: { sheet: 1, name: 1 },
    pin: { sheet: 1, name: 1 },
    one: { row: 'lib/RequestsPanel.svelte', sheet: 'lib/RequestsPanel.svelte' },
    marks: [
      { part: 'sheet', re: /\{\s*title:\s*'Soulseek',\s*rows:/, why: "the request sheet's Soulseek group (picks by user, its own Fetch)" },
      { part: 'name', re: /export function itemLine\(\w+: SoulseekItem\b/, why: 'the page\'s itemLine (Held and "waiting for X\'s other record" worked out again)' },
    ],
  },
  // The one word is editionsMark.
  Edition: {
    map: { row: 1, name: 2 },
    pin: { row: 1, name: 2 },
    marks: [
      { part: 'row', re: /editionsMark\(\w+\.editions\)\]\.filter\(Boolean\)\.join\(/, why: '"N editions" joined into a row\'s line in place (labelPage\'s rowLine)' },
      { part: 'name', re: /export (?:const|function) (?:withEditions|editionLines)\b/, why: 'edition words beside editionsMark (withEditions, editionLines)' },
    ],
  },
  // The crate sentence's term is one markup for every word kind (not told apart).
  Label: {
    map: { chip: 1 },
    pin: { chip: 1 },
    marks: [
      { part: 'chip', re: /<button class="chip" onclick=\{\(\) => openLabel\(/, why: 'AlbumAbout\'s label chip markup in place ("Warp · WARP123")' },
    ],
  },
  // 0.19's. Left unmarked as the one to be: the row, OutputSheet's player row; the chip, its head chip; the picture,
  // its cover and crown. Android's chip, sheet and name are out of the scan.
  'Play On target': {
    map: { row: 7, chip: 6, sheet: 4, name: 8, picture: 7 },
    pin: { row: 9, chip: 5, sheet: 3, name: 8, picture: 9 },
    held: true,
    one: { sheet: 'lib/OutputSheet.svelte', name: 'lib/playOn.svelte.ts' },
    marks: [
      { part: 'row', re: /label=\{`Follow \$\{/, why: "OutputSheet's follow rows (Follow X on Y)" },
      { part: 'row', re: /(?<=lead=\{[^}]*online[^}]*\? 'on' : 'off'\} )name=\{\w+\.name\}/, why: "a settings row of a player, its on/off dot before its name (PlayersSettings, KodiSettings, Diagnostics; Account's and Users' Devices, not in the map)" },
      { part: 'row', re: /\{#each playOn\.players\b[^}]*\}\s*<MenuEntry/, why: "a menu entry per player read in place (AuthPart's Guests ask picker)" },
      { part: 'row', re: /<label class="row">\s*<span>\{shown\(/, in: 'lib/GroupVolume.svelte', why: "GroupVolume's row per player" },
      { part: 'row', re: /<Item name="Where music plays"/, why: "DevicePart's Where music plays row" },
      { part: 'chip', re: /Playing on(?: \$\{|&nbsp;<\/span><strong>\{)\w+\.player/, why: '"Playing on X" (MiniPlayer\'s device button, NowPlaying\'s device chip, QueueSheet\'s head)' },
      { part: 'chip', re: /aria-label=\{`Play on: /, why: 'MiniPlayer\'s idle "Play on" button' },
      { part: 'chip', re: /<span class="on">\{' · '\}/, in: 'lib/QueuesSheet.svelte', why: "QueuesSheet's where-it-was-heard chip" },
      { part: 'sheet', re: /<MenuSheet label="Speaker delay"/, why: "PlayersSettings' speaker delay sheet" },
      { part: 'sheet', re: /<FormSheet title=\{[^}]*'Add Kodi'/, why: "KodiSettings' connection form" },
      { part: 'sheet', re: /<MenuSheet label="Guests ask"/, why: 'AuthPart\'s "Guests ask" player picker' },
      { part: 'name', re: /drivenName\(\) \?\? 'This device'/, why: "MiniPlayer's whereName (ignores groups)" },
      { part: 'name', re: /· \$\{owner\}'s`/, why: 'OutputSheet\'s "name · owner\'s"' },
      { part: 'name', re: /=== selfName\(\) \? 'This device'/, why: "GroupVolume's copy of outputName's This device" },
      { part: 'name', re: /appInfo\.player \?\? 'This device'/, why: "ModeSwitcher's device name" },
      { part: 'name', re: /appInfo\.player(?: \?\? '')?\)? : prefs\.browserName/, why: "this device's player name worked out again (DevicePart, playOnTarget's deviceName)" },
      { part: 'name', re: /`\$\{browser\} on \$\{os\}`/, why: 'playOnTarget\'s browser names ("Chrome on Android 2")' },
      { part: 'name', re: /\.played_on\b[^\n]*\.player\b/, why: "QueuesSheet's playedOn (an output's name, else its player's)" },
      { part: 'picture', re: /<Icon name="speaker"/, why: 'the hard-coded speaker icon' },
      { part: 'picture', re: /lead=\{[^}]*online[^}]*\? 'on' : 'off'\}/, why: 'the on/off dot (the same five settings rows)' },
    ],
  },
  // 0.19's. Left unmarked as the one to be: the row, QueueSheet's upcoming row; the chip, its Playing from. The name's one
  // is the server's name_of.
  Queue: {
    map: { row: 3, chip: 3, sheet: 4, name: 6, picture: 4 },
    pin: { row: 3, chip: 3, sheet: 4, name: 4, picture: 5 },
    held: true,
    one: { sheet: 'lib/QueueMenu.svelte', picture: 'lib/QueuesSheet.svelte' },
    marks: [
      { part: 'row', re: /\{#each previous as /, why: "QueueSheet's previous rows" },
      { part: 'row', re: /<li [^>]*class:asked=/, why: "QueueSheet's asked row in the queue" },
      { part: 'row', re: /\{#each queues\.list as /, why: "QueuesSheet's saved queue rows" },
      { part: 'chip', re: /<button class="[^"]*\bqueue\b[^"]*"/, why: 'a queue button of its own (MiniPlayer\'s; NowPlaying\'s with its own "i of n")' },
      { part: 'chip', re: /`\$\{\w+\} from Radio`/, why: "QueueMenu's track and lane counts" },
      { part: 'sheet', re: /role="dialog"\s+aria-label="Queue"/, why: 'QueueSheet, the queue sheet' },
      { part: 'sheet', re: /<MenuSheet\s+label="Queues"/, why: "QueuesSheet's inline per-queue menu" },
      { part: 'sheet', re: /label="New playlist from the queue"/, why: 'QueuePlaylistSheet' },
      { part: 'sheet', re: /label: [^,]*'Play again next'/, why: "TrackMenu's actions for a queue place" },
      { part: 'name', re: /same\(\(t\) => t\.album\) \?\? same\(\(t\) => t\.artist\)/, why: "the page's nameOf (the server's name_of again)" },
      { part: 'name', re: /\?\? 'Queue'\)/, why: "QueueMenu's name (from, else Queue)" },
      { part: 'name', re: /\{:else\}<strong>Queue<\/strong>/, why: "QueueSheet's name in its from line" },
      { part: 'name', re: /const line = \(\w+: QueueItem\) =>/, why: "QueuesSheet's line (Playing · n tracks · when)" },
      { part: 'picture', re: /\{ playlist: 'playlists', album: 'albums', favourites: 'heart'/, why: 'a copy of the FROM map (where the queue came from → icon)' },
      { part: 'picture', re: /fromView\.startsWith\('album:'\) \? 'albums'/, why: "QueueSheet's own from→icon map" },
      { part: 'picture', re: /aria-label="Queue"[^]{0,200}?<Icon name=(?:"playlists"|\{[^}]*'playlists'\})/, why: "a queue button's fixed icon (MiniPlayer, NowPlaying)" },
      { part: 'picture', re: /<MenuSheet[^]{0,80}?\sicon="list"/, why: "a queue sheet head's fixed icon (QueueMenu)" },
    ],
  },
  // Left unmarked as the one to be: the row, FileChanges' run row. The name's one is the server's named(); the
  // look-alike Undo toasts (not journaled) count as name sites.
  'Journal entry': {
    map: { row: 4, chip: 1, name: 8, picture: 2 },
    pin: { row: 5, chip: 1, name: 11, picture: 2 },
    one: { chip: 'lib/undo.svelte.ts', picture: 'lib/FileChanges.svelte' },
    marks: [
      { part: 'row', re: /<button\b[^\n]{0,100}?\bundo\w*\([^\n]{0,60}?>Undo<\/button>/, why: "an Undo written into a row of its own (FileChanges' record row, HealthPanel's intake row, ImportPart's playlist row)" },
      { part: 'row', re: /\.act === 'undo' && \w+ \? void undoRun\(/, why: "PV's files line (Its files written · Undo)" },
      { part: 'row', re: /<Item name=\{`\$\{live\.title\}/, in: 'lib/FileChanges.svelte', why: "the live run's row, the list's run row written again" },
      { part: 'chip', re: /toast\.show\(\{\s*text,[^]{0,40}?label:\s*'Undo'/, why: "whenWritten's own Undo toast (a second journal toast builder)" },
      { part: 'name', re: /export function withUndo\(text\b/, why: "withUndo's words, each caller's own (about 42 call sites, one way)" },
      { part: 'name', re: /export function whenWritten\(\w+: FileRun, text\b/, why: "whenWritten's words, each caller's own (4 call sites)" },
      { part: 'name', re: /export function runLine\(/, why: "runLine, a run's line" },
      { part: 'name', re: /export function stateLine\(/, why: "stateLine, a record's files line" },
      { part: 'name', re: /\? 'Undone' :/, why: '"Undone" said in place (FileChanges\' record, HealthPanel\'s intake row)' },
      { part: 'name', re: /\?\? 'A change to files'/, why: "Health's crumb naming a run itself" },
      { part: 'name', re: /toast\.show\(\{\s*text:\s*'[^']*',[^]{0,80}?label:\s*'Undo'/, why: "a look-alike Undo toast outside the journal (QueueMenu's shuffle and clear, QueueSheet's clear, port's New queue)" },
      { part: 'picture', re: /cover=\{albumCoverUrl\(i\.album\.source, i\.album\.id\)\}/, why: "a record's cover on HealthPanel's intake row" },
      { part: 'picture', re: /\.act === 'undo' \? 'check'/, why: "PV's files line picks its own state icons" },
    ],
  },
  // Left unmarked as the one to be: the name, trackIntake's itemLine.
  'Single-track intake item': {
    map: { row: 3, chip: 1, sheet: 1, name: 4, picture: 1 },
    pin: { row: 3, chip: 1, sheet: 1, name: 5, picture: 1 },
    one: { row: 'lib/ReplaceTrackSheet.svelte', chip: 'lib/ReplaceTrackSheet.svelte', sheet: 'lib/ReplaceTrackSheet.svelte', picture: 'lib/ReplaceTrackSheet.svelte' },
    marks: [
      { part: 'row', re: /\$\{\w+\.position\}\. \$\{\w+\.title\}/, why: 'the wanted track\'s "n. Title" written again (HealthItemTools)' },
      { part: 'row', re: /\.file\.split\(\/\[\\\\\/\]\/\)\.pop\(\) \?\? \w+\.file/, why: "a candidate's label written again (HealthPanel's A missing track)" },
      { part: 'row', re: /line=\{[^}\n]{0,40}\bitemLine\(\w+, picks\[/, why: "the item's status row (HealthPanel's review list)" },
      { part: 'chip', re: /chips=\{\[[^\]\n]{0,40}\bitemLine\(/, why: "the item's line as a chip in its sheet's head (HealthPanel)" },
      { part: 'sheet', re: /<MenuSheet label="A missing track"/, why: 'HealthPanel\'s "A missing track" sheet' },
      { part: 'name', re: /\bfetchLabel = \(\w+: Fetch\)/, why: 'fetchLabel, the same states worded beside itemLine ("Downloading N%")' },
      { part: 'name', re: /'The search did not finish\.?'/, in: 'lib/ReplaceTrackSheet.svelte', why: '"The search did not finish" written again' },
      { part: 'name', re: /export function againIn\(/, in: 'lib/trackIntake.ts', why: 'againIn copied from soulseekRules' },
      { part: 'name', re: /const clock = \(ms: number \| null\) =>/, in: 'lib/HealthItemTools.svelte', why: 'clock copied from ReplaceTrackSheet' },
      { part: 'name', re: /label="(?:Missing tracks|Find a better copy)…"/, why: "an entry's name hard-coded, not ENTRY_NAME's (AlbumMenu, TrackMenu)" },
      { part: 'picture', re: /cover=\{albumCoverUrl\(i\.album\.source, i\.album\.id\)\}/, why: "its album's cover on HealthPanel's row (ReplaceTrackSheet shows state icons)" },
    ],
  },
  // 0.19's. Left unmarked as the one to be: QueueSheet's asked row, its capped badge and its cover. The name's one is
  // the server's Asker::named.
  Ask: {
    map: { row: 1, chip: 2, sheet: 2, name: 3, picture: 1 },
    pin: { row: 1, chip: 2, sheet: 2, name: 3, picture: 1 },
    held: true,
    one: { sheet: 'lib/TrackMenu.svelte' },
    marks: [
      { part: 'row', re: /<ol class="rows" aria-label="Asked for">/, in: 'guest/Guest.svelte', why: "the guest's Asked for rows" },
      { part: 'chip', re: /<span class="asked-count"/, why: 'an uncapped asked count of its own (MiniPlayer, NowPlaying)' },
      { part: 'sheet', re: /label: 'Ask to play next'/, why: "the ask actions' own copy (AlbumMenu)" },
      { part: 'sheet', re: /label=\{`Always accept /, why: 'the always-accept switch worded again (OutputSheet)' },
      { part: 'name', re: /'user' in \w+\.from \? \w+\.from\.user : \w+\.from\.guest/, why: "the page's askerName (a guest's bare name, where the server says Guest and the name)" },
      { part: 'name', re: /\.state === 'accepted' \? '/, why: "answer words (the guest's stateText, following's toast)" },
      { part: 'picture', re: /\{#if \w+\.track\.cover\}/, why: "the guest's asked row's cover" },
    ],
  },
};

const SRC = new URL('../src/', import.meta.url);
/** Every page source by its path under web/src, read once. */
const sources = new Map<string, string>();
(function walk(dir: string) {
  for (const e of readdirSync(new URL(dir, SRC), { withFileTypes: true })) {
    const path = dir + e.name;
    if (e.isDirectory()) {
      if (path !== 'lib/generated') walk(path + '/');
    } else if (/\.(svelte|ts)$/.test(e.name)) sources.set(path, readFileSync(new URL(path, SRC), 'utf8').replace(/\r\n/g, '\n'));
  }
})('');

type Site = { at: string; line: string; why: string };
/** An object's sites by part: every match of its marks in the page sources, but in its one for that part. */
function sitesOf(o: Obj) {
  const by = new Map<Part, Site[]>(PARTS.map((p) => [p, []]));
  for (const m of o.marks) {
    const re = new RegExp(m.re.source, m.re.flags.includes('g') ? m.re.flags : m.re.flags + 'g');
    for (const [path, text] of m.in ? [[m.in, sources.get(m.in) ?? ''] as const] : sources) {
      if (o.one?.[m.part] === path) continue;
      for (const hit of text.matchAll(re)) {
        by.get(m.part)!.push({ at: `${path}@${hit.index}`, line: `${path}:${text.slice(0, hit.index).split('\n').length}`, why: m.why });
      }
    }
  }
  return by;
}
const found = new Map(Object.entries(OBJECTS).map(([name, o]) => [name, sitesOf(o)]));

test("every mark's file and every object's one is a page source", () => {
  for (const [name, o] of Object.entries(OBJECTS)) {
    for (const m of o.marks) if (m.in) assert.ok(sources.has(m.in), `${name}: ${m.in} is gone (${m.why}): follow its surface, or drop the mark`);
    for (const [part, file] of Object.entries(o.one ?? {})) assert.ok(sources.has(file), `${name}'s ${part}: its one, ${file}, is gone`);
  }
});

test('no site counts twice for one object', () => {
  for (const [name, by] of found) {
    const seen = new Map<string, string>();
    for (const s of [...by.values()].flat()) {
      assert.ok(!seen.has(s.at), `${name}: ${s.line} is matched by "${s.why}" and by "${seen.get(s.at)}"`);
      seen.set(s.at, s.why);
    }
  }
});

for (const [name, o] of Object.entries(OBJECTS)) {
  test(`${name}: drawn its own way as often as pinned${o.held ? ' (held for 0.19)' : ''}`, () => {
    for (const part of PARTS) {
      const sites = found.get(name)!.get(part)!;
      const pin = o.pin[part] ?? 0;
      const where = sites.map((s) => `\n  ${s.line}  ${s.why}`).join('');
      assert.ok(sites.length <= pin, `${name}'s ${part}: ${sites.length} own ways, pinned at ${pin}: draw it through the object's part.${where}`);
      assert.ok(sites.length >= pin, `${name}'s ${part}: ${sites.length} own ways, under its pin ${pin}: lower the pin with it.${where}`);
    }
  });
}
