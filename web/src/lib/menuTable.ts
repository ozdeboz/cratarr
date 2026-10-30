// What every menu holds, in what order (decision 0058, amended 2026-10-30: "overall consistency of our context menu
// population rules"): the table is this code. Each menu renders its cards from `cardsOf` and sorts its large actions
// with `inOrder`; what is true of the object (in the app, an administrator's, has tracks) decides which entries show,
// never where. No runes, so the tests take it as it is (tests/menu-table.test.ts holds every menu to it).

export type MenuKind = 'track' | 'album' | 'artist' | 'playlist' | 'crate' | 'genre' | 'queue' | 'selection' | 'bandcamp';

/** 0058's groups, B to H; A is the large actions. `library` is Change's second card (an album's library's own),
 *  `discover` Discover's own card (Want, Not interested; decisions 0070 and 0071), `extra` what a list's own items
 *  add to a selection's menu (several albums' Keep, Delete…), last. */
export type MenuGroupId = 'go' | 'discover' | 'feel' | 'collect' | 'take' | 'know' | 'change' | 'library' | 'remove' | 'extra';

export type EntryId =
  | 'view'
  | 'goAlbum'
  | 'goArtist'
  | 'goGenre'
  | 'soundsLike'
  | 'otherVersions'
  | 'discover'
  | 'favourite'
  | 'rating'
  | 'excludeEverywhere'
  | 'excludeArtist'
  | 'notInCrate'
  | 'addToPlaylist'
  | 'pin'
  | 'copyToMine'
  | 'saveAsPlaylist'
  | 'startCrate'
  | 'download'
  | 'keep'
  | 'removeFromDevice'
  | 'share'
  | 'sharedWithUsers'
  | 'openOnBandcamp'
  | 'artistOnBandcamp'
  | 'info'
  | 'webSearch'
  | 'geniusSearch'
  | 'rename'
  | 'spokenAs'
  | 'edit'
  | 'changeCover'
  | 'missingTracks'
  | 'classical'
  | 'deviceCover'
  | 'serverCover'
  | 'devicePicture'
  | 'albumPicture'
  | 'adjustBpm'
  | 'betterCopy'
  | 'alwaysAccept'
  | 'repair'
  | 'sync'
  | 'queues'
  | 'cancelConversion'
  | 'moveTo'
  | 'replaceIn'
  | 'removeFromQueue'
  | 'removeFromList'
  | 'removeResume'
  | 'clearQueue'
  | 'deleteAlbum'
  | 'deletePlaylist'
  | 'deleteCrate'
  | 'extra';

export type MenuCard = { group: MenuGroupId; entries: readonly EntryId[]; label?: string };
type Kind = { actions: readonly string[]; cards: readonly MenuCard[] };

/** The large actions' names in their order (0058's A): a menu shows the ones that apply. */
const PLAY = ['Play', 'Resume', 'Shuffle', 'Play next', 'Add to queue', 'Ask to play next', 'Ask to add to queue', 'Start radio'];

export const MENUS: Record<MenuKind, Kind> = {
  track: {
    // A queue row's own: Play now, and Play next moving it (or "Play again next", a copy, where it cannot move).
    actions: ['Play now', 'Play next', 'Play again next', 'Add to queue', 'Ask to play next', 'Ask to add to queue', 'Start radio'],
    cards: [
      { group: 'go', entries: ['goAlbum', 'goArtist', 'goGenre', 'soundsLike', 'otherVersions'] },
      { group: 'discover', entries: ['discover'] },
      // Its artists too (0.10, decision 0083): each credited one out everywhere, or out of the crate it is opened in.
      { group: 'feel', entries: ['favourite', 'excludeEverywhere', 'excludeArtist', 'notInCrate'] },
      { group: 'collect', entries: ['addToPlaylist'] },
      { group: 'take', entries: ['download', 'share'] },
      { group: 'know', entries: ['info', 'webSearch', 'geniusSearch'] },
      // A track asked for of one's own player (0.10): whoever asked, accepted at once from now on or not.
      { group: 'change', entries: ['adjustBpm', 'betterCopy', 'alwaysAccept'] },
      { group: 'remove', entries: ['removeFromQueue', 'removeFromList'] },
    ],
  },
  album: {
    actions: PLAY,
    cards: [
      { group: 'go', entries: ['view', 'goArtist', 'goGenre', 'soundsLike'] },
      { group: 'feel', entries: ['favourite', 'rating'] },
      { group: 'collect', entries: ['addToPlaylist'] },
      // Remove from this device beside what put it there (the owner, 2026-10-26: last in the menu it was out of sight).
      { group: 'take', entries: ['download', 'keep', 'removeFromDevice', 'share'] },
      { group: 'know', entries: ['webSearch', 'geniusSearch'] },
      { group: 'change', entries: ['changeCover', 'missingTracks', 'classical', 'deviceCover', 'serverCover'] },
      { group: 'library', label: 'Library', entries: ['cancelConversion', 'moveTo', 'replaceIn'] },
      { group: 'remove', entries: ['removeResume', 'deleteAlbum'] },
    ],
  },
  artist: {
    actions: ['Play', 'Shuffle', 'Play next', 'Add to queue', 'Start radio'],
    cards: [
      // Out everywhere, or out of the crate it is opened from (0.10, decision 0083).
      { group: 'feel', entries: ['favourite', 'excludeArtist', 'notInCrate'] },
      { group: 'collect', entries: ['addToPlaylist'] },
      { group: 'take', entries: ['download', 'keep', 'removeFromDevice', 'share'] },
      { group: 'know', entries: ['webSearch'] },
      { group: 'change', entries: ['devicePicture', 'albumPicture'] },
    ],
  },
  // Play next and Add to queue are not a playlist's, a crate's or a genre's (the owner, 2026-10-23).
  playlist: {
    actions: ['Play', 'Shuffle', 'Start radio'],
    cards: [
      { group: 'go', entries: ['view'] },
      { group: 'feel', entries: ['favourite'] },
      { group: 'collect', entries: ['pin', 'copyToMine'] },
      { group: 'take', entries: ['download', 'keep', 'share', 'sharedWithUsers'] },
      { group: 'know', entries: ['info'] },
      { group: 'change', entries: ['rename', 'spokenAs', 'repair', 'sync'] },
      { group: 'remove', entries: ['deletePlaylist'] },
    ],
  },
  crate: {
    actions: ['Play', 'Shuffle', 'Start radio'],
    cards: [
      { group: 'go', entries: ['view'] },
      // A crate of one's own is saved as a playlist; what sounds like something starts a crate of its own.
      { group: 'collect', entries: ['saveAsPlaylist', 'startCrate'] },
      { group: 'take', entries: ['download', 'keep', 'share'] },
      { group: 'change', entries: ['edit'] },
      { group: 'remove', entries: ['deleteCrate'] },
    ],
  },
  genre: {
    actions: ['Play', 'Shuffle', 'Start radio'],
    cards: [
      { group: 'go', entries: ['view'] },
      { group: 'collect', entries: ['startCrate'] },
      { group: 'take', entries: ['download', 'share'] },
    ],
  },
  // A queue's Shuffle and Repeat are at its header's end, not large actions.
  queue: {
    actions: [],
    cards: [
      { group: 'collect', entries: ['saveAsPlaylist'] },
      { group: 'take', entries: ['download'] },
      { group: 'change', entries: ['queues'] },
      { group: 'remove', entries: ['clearQueue'] },
    ],
  },
  selection: {
    actions: ['Play', 'Shuffle', 'Play next', 'Add to queue', 'Start radio'],
    cards: [
      { group: 'feel', entries: ['excludeEverywhere'] },
      { group: 'collect', entries: ['addToPlaylist'] },
      { group: 'take', entries: ['download'] },
      { group: 'change', entries: ['adjustBpm'] },
      { group: 'extra', entries: ['extra'] },
    ],
  },
  // Bandcamp's previews (decision 0071): queued like any track, never downloaded, kept or in a playlist; bought there.
  bandcamp: {
    actions: ['Play', 'Shuffle', 'Play next', 'Add to queue'],
    cards: [
      { group: 'go', entries: ['goArtist'] },
      { group: 'discover', entries: ['discover'] },
      { group: 'take', entries: ['openOnBandcamp', 'artistOnBandcamp', 'share'] },
    ],
  },
};

/** The groups' order, 0058's B to H with Discover's after Go to and a selection's extra last. */
export const GROUP_ORDER: readonly MenuGroupId[] = ['go', 'discover', 'feel', 'collect', 'take', 'know', 'change', 'library', 'remove', 'extra'];

/** Entries that render a card of their own (lib/DiscoverEntries, a selection's `extra`): not wrapped in another. */
export const OWN_CARD: readonly EntryId[] = ['discover', 'extra'];

/** The names that are the same everywhere (0058: Info not Details, View for the object, Go to for another); the
 *  entries named by what they are about ("Go to Bonobo", "Delete 3 playlists") are left out. */
export const ENTRY_NAME: Partial<Record<EntryId, string>> = {
  view: 'View',
  goAlbum: 'Go to album',
  soundsLike: 'Sounds like this',
  otherVersions: 'Other versions',
  favourite: 'Favourite',
  excludeEverywhere: 'Exclude everywhere',
  addToPlaylist: 'Add to playlist',
  copyToMine: 'Copy to my playlists',
  saveAsPlaylist: 'Save as playlist',
  startCrate: 'Start a crate from it',
  download: 'Download…',
  keep: 'Sync to this device',
  removeFromDevice: 'Remove from this device',
  share: 'Share',
  sharedWithUsers: 'Shared with other users',
  openOnBandcamp: 'Open on Bandcamp',
  info: 'Info',
  webSearch: 'Web search',
  geniusSearch: 'Genius search',
  rename: 'Rename',
  spokenAs: 'Spoken as…',
  edit: 'Edit',
  changeCover: 'Change cover',
  missingTracks: 'Missing tracks…',
  classical: 'Classical…',
  deviceCover: 'Change cover on this device',
  serverCover: "Use the server's cover",
  devicePicture: 'Change picture on this device',
  albumPicture: 'Use their album cover',
  adjustBpm: 'Adjust BPM',
  betterCopy: 'Find a better copy…',
  repair: 'Repair…',
  sync: 'Sync…',
  queues: 'Queues…',
  cancelConversion: 'Cancel conversion',
  removeFromQueue: 'Remove from queue',
  removeResume: 'Remove resume point',
  clearQueue: 'Clear the queue',
  deleteAlbum: 'Delete album',
  deleteCrate: 'Delete crate',
};

/** Names no menu uses any more: what 0058 and its amendment renamed. */
export const RETIRED_NAMES = ['Details', 'View album', 'Keep on this device', 'Download once', 'Export…'];

/** A kind's cards, in the table's order. */
export const cardsOf = (kind: MenuKind): readonly MenuCard[] => MENUS[kind].cards;

/** The large actions a menu offers, in the table's order (equals as given; one the table does not name, last). */
export function inOrder<T extends { label: string }>(kind: MenuKind, actions: readonly T[]): T[] {
  const order = MENUS[kind].actions;
  const at = (a: T) => {
    const i = order.indexOf(a.label);
    return i < 0 ? order.length : i;
  };
  return actions
    .map((a, i) => ({ a, i }))
    .sort((x, y) => at(x.a) - at(y.a) || x.i - y.i)
    .map((x) => x.a);
}
