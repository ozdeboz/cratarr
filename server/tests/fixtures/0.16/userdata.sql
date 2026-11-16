-- User data as 0.16.2 left it, for 0.17's step (FX01, written down as 0093 keeps no binaries in git): its tables as tag v0.16.2
-- makes them (a new user data's `sqlite_master`), then made-up rows in each table the step reads, under the records' ids
-- (`al-<record>`) of `catalogue.sql`, with 0.16's first start done.

CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE annotation (
  user TEXT NOT NULL, item TEXT NOT NULL, kind TEXT NOT NULL, pid TEXT,
  play_count INTEGER NOT NULL DEFAULT 0, play_date INTEGER, rating INTEGER NOT NULL DEFAULT 0,
  starred INTEGER NOT NULL DEFAULT 0, starred_at INTEGER, own_count INTEGER NOT NULL DEFAULT 0, own_date INTEGER,
  PRIMARY KEY (user, item, kind)) WITHOUT ROWID;

CREATE INDEX annotation_starred ON annotation (user, kind, starred, starred_at);

CREATE INDEX annotation_played ON annotation (user, kind, play_date);

CREATE INDEX annotation_plays ON annotation (user, kind, play_count);

CREATE INDEX annotation_pid ON annotation (pid);

CREATE TABLE play_queue (
  user TEXT PRIMARY KEY, songs TEXT NOT NULL, current TEXT, position_ms INTEGER NOT NULL DEFAULT 0,
  changed INTEGER NOT NULL, changed_by TEXT);

CREATE TABLE app_queue (
  user TEXT NOT NULL, client TEXT NOT NULL, songs TEXT NOT NULL, current INTEGER, position_ms INTEGER NOT NULL DEFAULT 0,
  changed INTEGER NOT NULL, PRIMARY KEY (user, client)) WITHOUT ROWID;

CREATE TABLE added (album TEXT PRIMARY KEY, at INTEGER NOT NULL, origin TEXT NOT NULL);

CREATE TABLE migration_report (part TEXT PRIMARY KEY, at INTEGER NOT NULL, report TEXT NOT NULL) WITHOUT ROWID;

CREATE TABLE album_review (album TEXT NOT NULL, folder INTEGER NOT NULL DEFAULT -1, base TEXT NOT NULL, state TEXT NOT NULL, at INTEGER NOT NULL, undo TEXT,
  name TEXT NOT NULL, album_artist TEXT NOT NULL, PRIMARY KEY (album, folder));

CREATE INDEX album_review_by_base ON album_review (base);

CREATE TABLE imported (
  account TEXT NOT NULL, source TEXT NOT NULL, server_user TEXT NOT NULL, kind TEXT NOT NULL, item TEXT NOT NULL,
  server_item TEXT, pid TEXT, plays INTEGER NOT NULL DEFAULT 0, played INTEGER, starred INTEGER NOT NULL DEFAULT 0,
  rating INTEGER NOT NULL DEFAULT 0, at INTEGER NOT NULL,
  PRIMARY KEY (account, source, server_user, kind, item)) WITHOUT ROWID;

CREATE INDEX imported_pid ON imported (pid);

CREATE TABLE import_choices (source TEXT PRIMARY KEY, choice TEXT NOT NULL);

CREATE TABLE bookmark (
  user TEXT NOT NULL, item TEXT NOT NULL, pid TEXT, position_ms INTEGER NOT NULL, comment TEXT NOT NULL DEFAULT '',
  created INTEGER NOT NULL, changed INTEGER NOT NULL, changed_by TEXT,
  PRIMARY KEY (user, item)) WITHOUT ROWID;

CREATE INDEX bookmark_pid ON bookmark (pid);

CREATE TABLE visual_snapshot (
  user TEXT NOT NULL, id TEXT NOT NULL, name TEXT NOT NULL, settings TEXT NOT NULL, created INTEGER NOT NULL, picture TEXT,
  PRIMARY KEY (user, id));

CREATE TABLE visual_rating (
  user TEXT NOT NULL, at INTEGER NOT NULL, a TEXT NOT NULL, b TEXT NOT NULL, choice TEXT NOT NULL, track TEXT,
  context TEXT);

CREATE TABLE visual_thumb (user TEXT NOT NULL, at INTEGER NOT NULL, thumb TEXT NOT NULL, body TEXT NOT NULL);

CREATE TABLE visual_limits (
  user TEXT PRIMARY KEY, version INTEGER NOT NULL, limits TEXT NOT NULL, at INTEGER NOT NULL);

CREATE TABLE user_settings (user TEXT PRIMARY KEY, version INTEGER NOT NULL, settings TEXT NOT NULL, at INTEGER NOT NULL);

CREATE TABLE plays (
  user TEXT NOT NULL, song TEXT NOT NULL, pid TEXT, at INTEGER NOT NULL, heard_secs REAL, length_secs REAL,
  ended TEXT NOT NULL, counted INTEGER NOT NULL, player TEXT, output TEXT, context TEXT, event TEXT, followed INTEGER NOT NULL DEFAULT 0);

CREATE INDEX plays_by_user ON plays (user, at);

CREATE UNIQUE INDEX plays_by_event ON plays (event) WHERE event IS NOT NULL;

CREATE TABLE scrobble_target (
  user TEXT NOT NULL, target TEXT NOT NULL, name TEXT NOT NULL, secret TEXT NOT NULL, broken TEXT, at INTEGER NOT NULL, off INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (user, target)) WITHOUT ROWID;

CREATE TABLE scrobble_queue (
  id INTEGER PRIMARY KEY, user TEXT NOT NULL, target TEXT NOT NULL, song TEXT NOT NULL, at INTEGER NOT NULL,
  tries INTEGER NOT NULL DEFAULT 0, next INTEGER NOT NULL DEFAULT 0, why TEXT, named TEXT);

CREATE INDEX scrobble_queue_due ON scrobble_queue (next, at);

CREATE TABLE discover_found (user TEXT PRIMARY KEY, at INTEGER NOT NULL, found TEXT NOT NULL) WITHOUT ROWID;

CREATE TABLE discover_around (user TEXT PRIMARY KEY, state TEXT NOT NULL) WITHOUT ROWID;

CREATE TABLE discover_answer (
  user TEXT NOT NULL, album TEXT NOT NULL, artist TEXT NOT NULL, answer TEXT NOT NULL, at INTEGER NOT NULL, name TEXT, artist_name TEXT, year INTEGER, cover TEXT,
  PRIMARY KEY (user, album)) WITHOUT ROWID;

CREATE TABLE request (
  id INTEGER PRIMARY KEY AUTOINCREMENT, user TEXT NOT NULL, album_key TEXT NOT NULL, artist_key TEXT NOT NULL,
  release_group TEXT, album TEXT NOT NULL, artist TEXT NOT NULL, year INTEGER, cover TEXT,
  at INTEGER NOT NULL, status TEXT NOT NULL, reason TEXT, decided_by TEXT, decided_at INTEGER, fetched TEXT);

CREATE INDEX request_status ON request (status, at);

CREATE TABLE discography (artist TEXT PRIMARY KEY, at INTEGER NOT NULL, groups TEXT NOT NULL) WITHOUT ROWID;

CREATE TABLE related_artists (artist TEXT PRIMARY KEY, at INTEGER NOT NULL, found TEXT NOT NULL) WITHOUT ROWID;

CREATE TABLE bandcamp_found (album TEXT PRIMARY KEY, at INTEGER NOT NULL, page TEXT) WITHOUT ROWID;

CREATE TABLE original_years (release TEXT PRIMARY KEY, year INTEGER, at INTEGER NOT NULL) WITHOUT ROWID;

CREATE TABLE release_types (key TEXT PRIMARY KEY, primary_type TEXT, secondary TEXT NOT NULL, at INTEGER NOT NULL, rule INTEGER NOT NULL DEFAULT 0) WITHOUT ROWID;

CREATE TABLE renumber_matched (album TEXT PRIMARY KEY, at INTEGER NOT NULL) WITHOUT ROWID;

CREATE TABLE rg_covers (rg TEXT PRIMARY KEY, url TEXT, at INTEGER NOT NULL) WITHOUT ROWID;

CREATE TABLE discogs_genres (album TEXT PRIMARY KEY, genres TEXT NOT NULL, at INTEGER NOT NULL) WITHOUT ROWID;

CREATE TABLE album_tags (album TEXT NOT NULL, tag TEXT NOT NULL, at INTEGER NOT NULL, PRIMARY KEY (album, tag)) WITHOUT ROWID;

CREATE INDEX album_tags_by_tag ON album_tags (tag, album);

CREATE TABLE words (
  subject TEXT NOT NULL, source TEXT NOT NULL, word TEXT NOT NULL, kind TEXT NOT NULL, weight REAL NOT NULL, at INTEGER NOT NULL,
  PRIMARY KEY (subject, source, word)) WITHOUT ROWID;

CREATE TABLE words_asked (subject TEXT NOT NULL, source TEXT NOT NULL, at INTEGER NOT NULL, ok INTEGER NOT NULL,
  PRIMARY KEY (subject, source)) WITHOUT ROWID;

CREATE TABLE album_matches (album TEXT PRIMARY KEY, at INTEGER NOT NULL, found TEXT) WITHOUT ROWID;

CREATE TABLE album_complete (album TEXT PRIMARY KEY, at INTEGER NOT NULL, found TEXT, discs INTEGER, rule INTEGER NOT NULL DEFAULT 0) WITHOUT ROWID;

CREATE TABLE discogs_facts (subject TEXT PRIMARY KEY, at INTEGER NOT NULL, facts TEXT) WITHOUT ROWID;

CREATE TABLE intake_heard (album TEXT NOT NULL, folder INTEGER NOT NULL DEFAULT -1, at INTEGER NOT NULL, heard TEXT NOT NULL, PRIMARY KEY (album, folder)) WITHOUT ROWID;

CREATE TABLE record_references (album TEXT PRIMARY KEY, chosen TEXT NOT NULL) WITHOUT ROWID;

CREATE TABLE track_words (track TEXT NOT NULL, kind TEXT NOT NULL, words TEXT NOT NULL, at INTEGER NOT NULL,
  PRIMARY KEY (track, kind)) WITHOUT ROWID;

CREATE TABLE mb_releases (release TEXT PRIMARY KEY, found INTEGER NOT NULL, facts TEXT NOT NULL, at INTEGER NOT NULL) WITHOUT ROWID;

CREATE TABLE mb_works (work TEXT PRIMARY KEY, title TEXT NOT NULL, composers TEXT NOT NULL, at INTEGER NOT NULL) WITHOUT ROWID;

CREATE TABLE mb_work_parents (work TEXT PRIMARY KEY, parent TEXT NOT NULL, title TEXT NOT NULL, number INTEGER, at INTEGER NOT NULL) WITHOUT ROWID;

CREATE TABLE artist_person (artist TEXT PRIMARY KEY, person TEXT, name TEXT, discogs INTEGER, at INTEGER NOT NULL DEFAULT 0) WITHOUT ROWID;

CREATE TABLE discogs_names (discogs INTEGER PRIMARY KEY, realname TEXT, aliases TEXT NOT NULL, at INTEGER NOT NULL);

CREATE TABLE discogs_link_genres (link TEXT PRIMARY KEY, genres TEXT NOT NULL, at INTEGER NOT NULL) WITHOUT ROWID;

CREATE TABLE classical_marks (album TEXT PRIMARY KEY, mark TEXT NOT NULL, at INTEGER NOT NULL) WITHOUT ROWID;

CREATE TABLE composer_keys (key TEXT PRIMARY KEY, name TEXT NOT NULL, alias_of TEXT, at INTEGER NOT NULL) WITHOUT ROWID;

CREATE TABLE record_overrides (album TEXT NOT NULL, field TEXT NOT NULL, value TEXT NOT NULL, at INTEGER NOT NULL,
  PRIMARY KEY (album, field)) WITHOUT ROWID;

CREATE TABLE play_seen (id TEXT PRIMARY KEY, at INTEGER NOT NULL) WITHOUT ROWID;

CREATE INDEX play_seen_at ON play_seen (at);

CREATE TABLE kept_answer (
  user TEXT NOT NULL, key TEXT NOT NULL, asked TEXT NOT NULL, status INTEGER NOT NULL, kind TEXT, body BLOB NOT NULL,
  at INTEGER NOT NULL, PRIMARY KEY (user, key)) WITHOUT ROWID;

CREATE INDEX kept_answer_at ON kept_answer (at);

CREATE TABLE change_at (
  account TEXT NOT NULL, kind TEXT NOT NULL, item TEXT NOT NULL, at INTEGER NOT NULL,
  PRIMARY KEY (account, kind, item)) WITHOUT ROWID;

CREATE TABLE change_log (
  rev INTEGER PRIMARY KEY AUTOINCREMENT, account TEXT NOT NULL, kind TEXT NOT NULL, item TEXT NOT NULL, op TEXT NOT NULL,
  at INTEGER NOT NULL, shared INTEGER NOT NULL DEFAULT 0, seen_by TEXT NOT NULL DEFAULT '');

CREATE TABLE change_log_floor (one INTEGER PRIMARY KEY CHECK (one = 1), rev INTEGER NOT NULL);

CREATE TABLE playlist_op (account TEXT NOT NULL, op_id TEXT NOT NULL, at INTEGER NOT NULL, why TEXT,
  PRIMARY KEY (account, op_id)) WITHOUT ROWID;

CREATE INDEX playlist_op_at ON playlist_op (at);

CREATE UNIQUE INDEX change_log_one ON change_log (account, kind, item, seen_by);

CREATE TABLE playlist (
  id TEXT PRIMARY KEY, owner TEXT NOT NULL, name TEXT NOT NULL, shared INTEGER NOT NULL DEFAULT 0,
  created INTEGER NOT NULL, changed INTEGER NOT NULL, gone INTEGER) WITHOUT ROWID;

CREATE UNIQUE INDEX playlist_named ON playlist (owner, name) WHERE gone IS NULL;

CREATE TABLE playlist_item (
  playlist TEXT NOT NULL, entry TEXT NOT NULL, n INTEGER, line TEXT NOT NULL, track TEXT, added INTEGER,
  placed INTEGER NOT NULL DEFAULT 0, gone INTEGER, after TEXT, PRIMARY KEY (playlist, entry)) WITHOUT ROWID;

CREATE INDEX playlist_item_gone ON playlist_item (gone);

CREATE TABLE playlist_moved (old TEXT PRIMARY KEY, id TEXT NOT NULL) WITHOUT ROWID;

CREATE TABLE playlist_import (one INTEGER PRIMARY KEY CHECK (one = 1), at INTEGER NOT NULL, found INTEGER NOT NULL);

CREATE TABLE preference (
  account TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL, changed INTEGER NOT NULL,
  PRIMARY KEY (account, key)) WITHOUT ROWID;

CREATE TABLE scrobble_seen (
  user TEXT NOT NULL, id TEXT NOT NULL, at INTEGER NOT NULL, PRIMARY KEY (user, id)) WITHOUT ROWID;

CREATE INDEX scrobble_seen_at ON scrobble_seen (at);

CREATE INDEX annotation_own_played ON annotation (user, kind, own_date);

CREATE TABLE excluded_artist (
           user TEXT NOT NULL, key TEXT NOT NULL, name TEXT NOT NULL, at INTEGER NOT NULL,
           PRIMARY KEY (user, key)) WITHOUT ROWID;

-- The rows: made up (0093), each table 0.17's step reads in the shapes production's have (read 2026-11-16). MusicBrainz ids
-- are 00000000-0000-4000-8000-000000000NNN: releases from 001, release groups from 101, artists from 201, recordings from 301.
-- Discogs' are 1000NNN for masters and 2000NNN for releases.

-- 0.14's choices (`app::references::Chosen`), under the records' ids since 0.16: "Use this", a link pasted by hand, a guess
-- confirmed with nothing chosen, AcoustID counting (it has no list) with links as 0.16 kept them (Discogs' older link to a
-- master, a MusicBrainz release's id cut short) and a guess confirmed it has none of, and a retired record's.
INSERT INTO record_references (album, chosen) VALUES
  ('al-1', '{"counting":"musicbrainz","by_hand":false,"links":{},"confirmed":["musicbrainz"]}'),
  ('al-2', '{"counting":"discogs","by_hand":true,"links":{"discogs":"https://www.discogs.com/release/2000002-Noor-Field-Two-Rooms","musicbrainz":"https://musicbrainz.org/release/00000000-0000-4000-8000-000000000002"},"confirmed":[]}'),
  ('al-3', '{"counting":null,"by_hand":false,"links":{},"confirmed":["bandcamp"]}'),
  ('al-5', '{"counting":"acoustid","by_hand":false,"links":{"discogs":"https://www.discogs.com/master/view/1000005","musicbrainz":"https://musicbrainz.org/release/00000000-0000-4000-8000-00000000000"},"confirmed":["bandcamp"]}'),
  ('al-6', '{"counting":"musicbrainz","by_hand":true,"links":{"musicbrainz":"https://musicbrainz.org/release/00000000-0000-4000-8000-000000000006"},"confirmed":["discogs"]}');

-- Matching's finds (`app::matching::Match`): certain (Signal's with nothing chosen), likely with a length MusicBrainz lacks,
-- none, two copies' certain matches on one release, one whose release names each track "(Remastered)" as the files do not,
-- and one under an id 0.16's first start could not move (a row no list matched stays as it was).
INSERT INTO album_matches (album, at, found) VALUES
  ('al-1', 1793900000, '{"release":"00000000-0000-4000-8000-000000000001","release_group":"00000000-0000-4000-8000-000000000101","title":"Tide Tables","artist":"Glass Harbour","artist_id":"00000000-0000-4000-8000-000000000201","year":2012,"certain":true,"why":"Tide Tables, 2012, CD · all 6 tracks within 2 s","tracks":[["01 Slack Water.flac","00000000-0000-4000-8000-000000000301","00000000-0000-4000-8000-000000000201"],["02 Neap.flac","00000000-0000-4000-8000-000000000302","00000000-0000-4000-8000-000000000201"],["03 Spring Tide.flac","00000000-0000-4000-8000-000000000303","00000000-0000-4000-8000-000000000201"],["04 Ebb.flac","00000000-0000-4000-8000-000000000304","00000000-0000-4000-8000-000000000201"],["05 Flood.flac","00000000-0000-4000-8000-000000000305","00000000-0000-4000-8000-000000000201"],["06 Low Water.flac","00000000-0000-4000-8000-000000000306","00000000-0000-4000-8000-000000000201"]],"lengths":[181,203,240,198,215,262]}'),
  ('al-3', 1793900000, '{"release":"00000000-0000-4000-8000-000000000003","release_group":"00000000-0000-4000-8000-000000000103","title":"Night Walks","artist":"Kite Theory","artist_id":"00000000-0000-4000-8000-000000000203","year":2020,"certain":false,"why":"Night Walks, 2020, Digital Media · 3 of 3 tracks within 5 s","tracks":[["01 Lamplight.mp3","00000000-0000-4000-8000-000000000311","00000000-0000-4000-8000-000000000203"],["02 Crossing.mp3","00000000-0000-4000-8000-000000000312","00000000-0000-4000-8000-000000000203"],["03 Home.mp3","00000000-0000-4000-8000-000000000313","00000000-0000-4000-8000-000000000203"]],"lengths":[200,null,187]}'),
  ('al-4', 1793900000, NULL),
  ('al-9', 1794600200, '{"release":"00000000-0000-4000-8000-000000000002","release_group":"00000000-0000-4000-8000-000000000102","title":"Two Rooms","artist":"Noor Field","artist_id":"00000000-0000-4000-8000-000000000202","year":2019,"certain":true,"why":"Two Rooms, 2019, CD · all 6 tracks within 2 s","tracks":[],"lengths":[]}'),
  ('al-7', 1794300000, '{"release":"00000000-0000-4000-8000-000000000007","release_group":"00000000-0000-4000-8000-000000000107","title":"Signal","artist":"Noor Field","artist_id":"00000000-0000-4000-8000-000000000202","year":2021,"certain":true,"why":"Signal, 2021, Digital Media · all 1 tracks within 2 s","tracks":[["01 Signal.flac","00000000-0000-4000-8000-000000000371","00000000-0000-4000-8000-000000000202"]],"lengths":[215]}'),
  ('al-10', 1794700050, '{"release":"00000000-0000-4000-8000-000000000020","release_group":"00000000-0000-4000-8000-000000000120","title":"Ferry Lights","artist":"Kite Theory","artist_id":"00000000-0000-4000-8000-000000000203","year":2016,"certain":true,"why":"Ferry Lights, 2016, CD · all 2 tracks within 2 s","tracks":[],"lengths":[]}'),
  ('al-11', 1794700150, '{"release":"00000000-0000-4000-8000-000000000020","release_group":"00000000-0000-4000-8000-000000000120","title":"Ferry Lights","artist":"Kite Theory","artist_id":"00000000-0000-4000-8000-000000000203","year":2016,"certain":true,"why":"Ferry Lights, 2016, CD · all 2 tracks within 2 s","tracks":[],"lengths":[]}'),
  ('al-12', 1794800050, '{"release":"00000000-0000-4000-8000-000000000030","release_group":"00000000-0000-4000-8000-000000000130","title":"Undertow","artist":"Orbit Lane","artist_id":"00000000-0000-4000-8000-000000000205","year":2018,"certain":true,"why":"Undertow, 2018, Digital Media · all 3 tracks within 2 s","tracks":[],"lengths":[]}'),
  ('al-7Qm2VxRb9KfPz3LcWnTd5h', 1780000000, '{"release":"00000000-0000-4000-8000-000000000010","release_group":"00000000-0000-4000-8000-000000000110","title":"Junk Dates","artist":"Orbit Lane","artist_id":"00000000-0000-4000-8000-000000000205","year":2010,"certain":true,"why":"Junk Dates, 2010, CD · all 2 tracks within 2 s","tracks":[["01 Zero.flac","00000000-0000-4000-8000-000000000341",null],["02 One.flac","00000000-0000-4000-8000-000000000342",null]],"lengths":[180,175]}');

-- What Discogs said (`app::discogs::AlbumFacts`): an album's by `album:<record>`, with a master, with a release and no master,
-- a search by names that found another record's release (Undated's: Night Walks'), asked in vain; and a release group of
-- Discover's (`rg:`), which is no album's.
INSERT INTO discogs_facts (subject, at, facts) VALUES
  ('album:al-1', 1793900000, '{"master":1000001,"release":2000001,"year":2012,"genres":["Electronic"],"styles":["Dub Techno","Ambient"],"labels":[{"name":"Low Tide Records","catno":"LTR-012"}],"credits":[{"name":"Glass Harbour","role":"Written-By"},{"name":"Pressing Plant Mastering","role":"Mastered By","tracks":"1 to 6"}],"url":"https://www.discogs.com/master/1000001"}'),
  ('album:al-2', 1793900000, '{"master":null,"release":2000002,"year":2019,"genres":["Electronic"],"styles":["Minimal"],"labels":[{"name":"Not On Label (Noor Field Self-released)","catno":"none"}],"credits":[],"url":"https://www.discogs.com/release/2000002"}'),
  ('album:al-4', 1793900000, '{"master":null,"release":2000003,"year":2020,"genres":["Electronic"],"styles":["Ambient"],"labels":[{"name":"Paper Lantern","catno":"PL-03"}],"credits":[],"url":"https://www.discogs.com/release/2000003"}'),
  ('album:al-5', 1793900000, NULL),
  ('album:al-6', 1780000000, '{"master":1000006,"release":2000006,"year":2011,"genres":["Electronic"],"styles":["Ambient"],"labels":[{"name":"Low Tide Records","catno":"LTR-009"}],"credits":[],"url":"https://www.discogs.com/master/1000006"}'),
  ('rg:00000000-0000-4000-8000-000000000112', 1793950000, '{"master":1000012,"release":2000012,"year":2016,"genres":["Electronic"],"styles":["Ambient"],"labels":[{"name":"Low Tide Records","catno":"LTR-020"}],"credits":[],"url":"https://www.discogs.com/master/1000012"}');

-- Albums found on Bandcamp (0.9.3), by release group, else `key(artist)|key(album)`: an owned record's by its group and by
-- its names, a miss, a record not had (a request's), a page by names a retired record and a live one share, and an artist's
-- page, which is no album's.
INSERT INTO bandcamp_found (album, at, page) VALUES
  ('00000000-0000-4000-8000-000000000101', 1793900000, 'https://glassharbour.bandcamp.com/album/tide-tables'),
  ('kitetheory|nightwalksep', 1793900000, 'https://kitetheory.bandcamp.com/album/night-walks'),
  ('kitetheory|undated', 1793900000, NULL),
  ('00000000-0000-4000-8000-000000000111', 1793950000, 'https://glassharbour.bandcamp.com/album/harbour-lights'),
  ('glassharbour|partialshore', 1794600000, 'https://glassharbour.bandcamp.com/album/partial-shore'),
  ('noorfield|signal', 1794300000, 'https://noorfield.bandcamp.com/music');

-- Releases the works' pass read (`app::works::Stored`), with MusicBrainz's verified Discogs links: a release and its master,
-- a release alone, one it links to nothing on Discogs, and one MusicBrainz did not know.
INSERT INTO mb_releases (release, found, facts, at) VALUES
  ('00000000-0000-4000-8000-000000000001', 1, '{"genres":["ambient","dub techno"],"discogs":["release:2000001","master:1000001"],"performers":[],"performed":false,"tracks":[],"with_parents":true}', 1793900000),
  ('00000000-0000-4000-8000-000000000003', 1, '{"genres":[],"discogs":["release:2000003"],"performers":[],"performed":false,"tracks":[],"with_parents":true}', 1793900000),
  ('00000000-0000-4000-8000-000000000007', 1, '{"genres":[],"discogs":[],"performers":[],"performed":false,"tracks":[],"with_parents":true}', 1794300000),
  ('00000000-0000-4000-8000-000000000099', 0, '{}', 1793900000);

-- Discover's discographies (`app::discover::Group`), by the artist's MusicBrainz id: one read before 0.9.17 kept dates, and a
-- related artist's.
INSERT INTO discography (artist, at, groups) VALUES
  ('00000000-0000-4000-8000-000000000201', 1793900000, '[{"id":"00000000-0000-4000-8000-000000000106","title":"Partial Shore","kind":"Album","year":2011,"genres":[],"date":"2011"},{"id":"00000000-0000-4000-8000-000000000101","title":"Tide Tables","kind":"Album","year":2012,"genres":["ambient","dub techno"],"date":"2012-03-05"},{"id":"00000000-0000-4000-8000-000000000111","title":"Harbour Lights","kind":"EP","year":2014,"genres":["ambient"],"date":"2014-06"},{"id":"00000000-0000-4000-8000-000000000112","title":"Deep Water","kind":"Album","year":2016,"genres":["ambient"],"date":"2016-09-30"}]'),
  ('00000000-0000-4000-8000-000000000202', 1793900000, '[{"id":"00000000-0000-4000-8000-000000000102","title":"Two Rooms","kind":"Album","year":2019,"genres":["electronic"],"date":"2019-04-12"},{"id":"00000000-0000-4000-8000-000000000114","title":"Open Plan","kind":"Album","year":2023,"genres":["electronic","minimal"],"date":"2023-02-03"}]'),
  ('00000000-0000-4000-8000-000000000203', 1780000000, '[{"id":"00000000-0000-4000-8000-000000000103","title":"Night Walks","kind":"EP","year":2020,"genres":[]},{"id":"00000000-0000-4000-8000-000000000113","title":"Paper Kites","kind":"Album","year":2018,"genres":[]}]'),
  ('00000000-0000-4000-8000-000000000204', 1793950000, '[{"id":"00000000-0000-4000-8000-000000000115","title":"Lost Tapes","kind":"Album","year":1999,"genres":[],"date":"1999"},{"id":"00000000-0000-4000-8000-000000000116","title":"Long Player","kind":"Album","year":2015,"genres":["rock"],"date":"2015-05-01"}]');

-- Requests (0070), by names: with a release group (Soulseek fetches those, `soulseek.json`), one for a record owned by its
-- group; without one (production: 8 of 87), owned by names, not owned, and cancelled.
INSERT INTO request (id, user, album_key, artist_key, release_group, album, artist, year, cover, at, status, reason, decided_by, decided_at, fetched) VALUES
  (1, 'ann', '00000000-0000-4000-8000-000000000112', 'glass harbour', '00000000-0000-4000-8000-000000000112', 'Deep Water', 'Glass Harbour', 2016, '/api/covers/release-group/00000000-0000-4000-8000-000000000112', 1793950000, 'approved', NULL, 'ann', 1793950000, 'Soulseek searches for it'),
  (2, 'bob', '00000000-0000-4000-8000-000000000111', 'glass harbour', '00000000-0000-4000-8000-000000000111', 'Harbour Lights', 'Glass Harbour', 2014, '/api/covers/release-group/00000000-0000-4000-8000-000000000111', 1793950100, 'approved', NULL, 'ann', 1793960000, 'Soulseek searches for it'),
  (3, 'bob', '00000000-0000-4000-8000-000000000113', 'kite theory', '00000000-0000-4000-8000-000000000113', 'Paper Kites', 'Kite Theory', 2018, '/api/covers/release-group/00000000-0000-4000-8000-000000000113', 1793950200, 'approved', NULL, 'ann', 1793960000, 'Soulseek searches for it'),
  (4, 'ann', '00000000-0000-4000-8000-000000000114', 'noor field', '00000000-0000-4000-8000-000000000114', 'Open Plan', 'Noor Field', 2023, '/api/covers/release-group/00000000-0000-4000-8000-000000000114', 1793950300, 'approved', NULL, 'ann', 1793950300, 'Soulseek searches for it'),
  (5, 'ann', '00000000-0000-4000-8000-000000000115', 'tape club', '00000000-0000-4000-8000-000000000115', 'Lost Tapes', 'Tape Club', 1999, NULL, 1793950400, 'approved', NULL, 'ann', 1793950400, 'Soulseek searches for it'),
  (6, 'ann', '00000000-0000-4000-8000-000000000101', 'glass harbour', '00000000-0000-4000-8000-000000000101', 'Tide Tables', 'Glass Harbour', 2012, '/api/covers/release-group/00000000-0000-4000-8000-000000000101', 1700000000, 'approved', NULL, 'ann', 1700000000, 'Lidarr added it and is looking for it'),
  (7, 'bob', 'noorfield|tworooms', 'noor field', NULL, 'Two Rooms', 'Noor Field', 2019, 'https://example.org/covers/two-rooms.jpg', 1750000000, 'approved', NULL, 'ann', 1750000000, 'Not handed over to Soulseek: no release group to judge it by'),
  (8, 'bob', 'orbitlane|quietlanes', 'orbit lane', NULL, 'Quiet Lanes', 'Orbit Lane', 2017, 'https://example.org/covers/quiet-lanes.jpg', 1793950500, 'approved', NULL, 'ann', 1793960000, 'Not handed over to Soulseek: no release group to judge it by'),
  (9, 'ann', 'tapeclub|nowherefast', 'tape club', NULL, 'Nowhere Fast', 'Tape Club', NULL, NULL, 1793950600, 'cancelled', NULL, NULL, NULL, NULL),
  (10, 'bob', '00000000-0000-4000-8000-000000000116', 'tape club', '00000000-0000-4000-8000-000000000116', 'Long Player', 'Tape Club', 2015, '/api/covers/release-group/00000000-0000-4000-8000-000000000116', 1793950700, 'approved', NULL, 'ann', 1793950700, 'Soulseek searches for it');

-- 0.16's first start, done: what says it is over is this part of the report.
INSERT INTO migration_report (part, at, report) VALUES
  ('records', 1794100000, '{"records":6,"copies":6,"tracks_merged":0,"bases_on_two_records":0,"artists":6,"shapes_moved":0,"shapes_left":0,"beats_moved":0,"beats_left":0,"tables":[],"unmatched":[{"what":"album_matches","count":1,"sample":["al-7Qm2VxRb9KfPz3LcWnTd5h"]}],"differed":[]}');
