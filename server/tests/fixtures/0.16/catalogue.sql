-- A catalogue as 0.16.2 left it, for 0.17's step (FX01, written down as 0093 keeps no binaries in git): its tables as tag v0.16.2
-- makes them (a new catalogue's `sqlite_master`, the triggers left out, as each opening writes them again), then made-up
-- records and their tracks after 0.16's first start. No library is behind it: there are no files.

CREATE TABLE folders (id INTEGER PRIMARY KEY, path TEXT NOT NULL, home INTEGER, was TEXT);

CREATE TABLE files (
  folder INTEGER NOT NULL, rel TEXT NOT NULL, size INTEGER NOT NULL, mtime INTEGER NOT NULL,
  state INTEGER NOT NULL DEFAULT 0, format TEXT, bitrate INTEGER, duration_ms INTEGER, error TEXT, issue TEXT, issue_detail TEXT, track INTEGER, gone_at INTEGER, trashed TEXT, bits INTEGER, rate INTEGER, came INTEGER,
  PRIMARY KEY (folder, rel)) WITHOUT ROWID;

CREATE TABLE cover_files (
  folder INTEGER NOT NULL, rel TEXT NOT NULL, size INTEGER NOT NULL, mtime INTEGER NOT NULL,
  PRIMARY KEY (folder, rel)) WITHOUT ROWID;

CREATE TABLE tags (
  folder INTEGER NOT NULL, rel TEXT NOT NULL, n INTEGER NOT NULL, raw TEXT NOT NULL, name TEXT, value TEXT NOT NULL,
  PRIMARY KEY (folder, rel, n)) WITHOUT ROWID;

CREATE TABLE source_ids (
  source TEXT NOT NULL, track_id TEXT NOT NULL, library TEXT NOT NULL, folder INTEGER NOT NULL, rel TEXT NOT NULL,
  seen INTEGER NOT NULL, PRIMARY KEY (source, track_id)) WITHOUT ROWID;

CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE handovers (
  folder INTEGER NOT NULL, rel TEXT NOT NULL, track INTEGER NOT NULL, at INTEGER NOT NULL, replaces TEXT,
  PRIMARY KEY (folder, rel)) WITHOUT ROWID;

CREATE TABLE tracks (id INTEGER PRIMARY KEY AUTOINCREMENT, born INTEGER NOT NULL, retired_at INTEGER, record INTEGER, disc INTEGER, number INTEGER, back INTEGER, was_rel TEXT, was_ms INTEGER, was_title TEXT, was_artist TEXT, was_album TEXT, was_recording TEXT);

CREATE TABLE track_moved (old INTEGER PRIMARY KEY, id INTEGER NOT NULL, followed INTEGER NOT NULL DEFAULT 0);

CREATE TABLE records (
  id INTEGER PRIMARY KEY AUTOINCREMENT, born INTEGER NOT NULL, retired_at INTEGER,
  title TEXT, artist TEXT, year INTEGER, kind TEXT, counting TEXT);

CREATE TABLE artists (
  id INTEGER PRIMARY KEY AUTOINCREMENT, born INTEGER NOT NULL, name TEXT NOT NULL, sort TEXT, key TEXT NOT NULL);

CREATE TABLE artist_moved (old TEXT PRIMARY KEY, id INTEGER NOT NULL) WITHOUT ROWID;

CREATE TABLE album_moved (
  old TEXT NOT NULL, form TEXT NOT NULL, record INTEGER NOT NULL, folder INTEGER, rank INTEGER NOT NULL,
  PRIMARY KEY (old, form, record)) WITHOUT ROWID;

CREATE TABLE analysis (
  folder INTEGER NOT NULL, rel TEXT NOT NULL, size INTEGER NOT NULL, mtime INTEGER NOT NULL, version INTEGER NOT NULL,
  duration_secs REAL, loudness_lufs REAL, track_gain_db REAL, track_peak REAL, bpm REAL, bpm_confidence REAL,
  waveform BLOB, blocks TEXT, analysed_at INTEGER NOT NULL, complete INTEGER, four REAL,
  PRIMARY KEY (folder, rel)) WITHOUT ROWID;

CREATE TABLE analysis_shape (
  folder INTEGER NOT NULL, rel TEXT NOT NULL, waveform BLOB, blocks BLOB, beats BLOB, bands BLOB, beats_by INTEGER, band_wave BLOB, spectrum BLOB,
  PRIMARY KEY (folder, rel));

CREATE TABLE audiomuse (
  track INTEGER PRIMARY KEY, energy REAL, key TEXT, style TEXT, moods TEXT, embedding BLOB);

CREATE TABLE beat_this (
  folder INTEGER NOT NULL, rel TEXT NOT NULL, size INTEGER NOT NULL, mtime INTEGER NOT NULL,
  beats BLOB NOT NULL, downbeats BLOB NOT NULL, at INTEGER NOT NULL, meter INTEGER, meter_sure REAL,
  PRIMARY KEY (folder, rel)) WITHOUT ROWID;

CREATE INDEX source_ids_by_file ON source_ids (folder, rel);

CREATE INDEX artists_by_key ON artists (key);

CREATE UNIQUE INDEX album_moved_one_record ON album_moved (old) WHERE form = 'id';

CREATE INDEX files_by_track ON files (track);

CREATE INDEX tracks_by_record ON tracks (record);

CREATE TABLE index_log (rev INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, item INTEGER NOT NULL);

CREATE UNIQUE INDEX index_log_one ON index_log (kind, item);

CREATE TABLE index_prints (
  kind TEXT NOT NULL, item INTEGER NOT NULL, print INTEGER NOT NULL, PRIMARY KEY (kind, item)) WITHOUT ROWID;

CREATE TABLE index_floor (one INTEGER PRIMARY KEY CHECK (one = 1), rev INTEGER NOT NULL);

-- The rows: made up (0093), as 0.16.2's first start and the passes after it left them. 0.16 wrote no record's `kind` or
-- `counting`. Records 1 to 5 are had; 6 is retired, its tracks retired with it; 7 came after the first start; 8 is Partial
-- Shore come back; 9 a second copy of Two Rooms; 10 and 11 two copies of one release, the younger's key the later; 12 a record
-- whose certain match is a remaster's.
INSERT INTO records (id, born, retired_at, title, artist, year, kind, counting) VALUES
  (1, 1760000000, NULL, 'Tide Tables', 'Glass Harbour', 2012, NULL, NULL),
  (2, 1760000100, NULL, 'Two Rooms', 'Noor Field', 2019, NULL, NULL),
  (3, 1760000200, NULL, 'Night Walks EP', 'Kite Theory', 2020, NULL, NULL),
  (4, 1760000300, NULL, 'Undated', 'Kite Theory', NULL, NULL, NULL),
  (5, 1760000400, NULL, 'Late Train OST', 'Various Artists', 2005, NULL, NULL),
  (6, 1760000500, 1794500000, 'Partial Shore', 'Glass Harbour', 2011, NULL, NULL),
  (7, 1794200000, NULL, 'Signal', 'Noor Field', 2021, NULL, NULL),
  (8, 1794600000, NULL, 'Partial Shore', 'Glass Harbour', 2011, NULL, NULL),
  (9, 1794600100, NULL, 'Two Rooms', 'Noor Field', 2019, NULL, NULL),
  (10, 1794700000, NULL, 'Ferry Lights', 'Kite Theory', 2016, NULL, NULL),
  (11, 1794700100, NULL, 'Ferry Lights', 'Kite Theory', 2016, NULL, NULL),
  (12, 1794800000, NULL, 'Undertow', 'Orbit Lane', 2018, NULL, NULL);
INSERT INTO tracks (id, born, retired_at, record, disc, number) VALUES
  (1, 1760000000, NULL, 1, 1, 1), (2, 1760000000, NULL, 1, 1, 2), (3, 1760000000, NULL, 1, 1, 3),
  (4, 1760000000, NULL, 1, 1, 4), (5, 1760000000, NULL, 1, 1, 5), (6, 1760000000, NULL, 1, 1, 6),
  (7, 1760000100, NULL, 2, 1, 1), (8, 1760000100, NULL, 2, 1, 2), (9, 1760000100, NULL, 2, 1, 3),
  (10, 1760000100, NULL, 2, 2, 1), (11, 1760000100, NULL, 2, 2, 2), (12, 1760000100, NULL, 2, 2, 3),
  (13, 1760000200, NULL, 3, 1, 1), (14, 1760000200, NULL, 3, 1, 2), (15, 1760000200, NULL, 3, 1, 3),
  (16, 1760000300, NULL, 4, 1, 1), (17, 1760000300, NULL, 4, 1, 2), (18, 1760000300, NULL, 4, 1, 3), (19, 1760000300, NULL, 4, 1, 4),
  (20, 1760000400, NULL, 5, 1, 1), (21, 1760000400, NULL, 5, 1, 2), (22, 1760000400, NULL, 5, 1, 3), (23, 1760000400, NULL, 5, 1, 4), (24, 1760000400, NULL, 5, 1, 5),
  (28, 1794200000, NULL, 7, 1, 1),
  (29, 1794600100, NULL, 9, 1, 1), (30, 1794600100, NULL, 9, 1, 2), (31, 1794600100, NULL, 9, 1, 3),
  (32, 1794600100, NULL, 9, 2, 1), (33, 1794600100, NULL, 9, 2, 2), (34, 1794600100, NULL, 9, 2, 3),
  (35, 1794600000, NULL, 8, 1, 1), (36, 1794600000, NULL, 8, 1, 2), (37, 1794600000, NULL, 8, 1, 5),
  (38, 1794700000, NULL, 10, 1, 1), (39, 1794700000, NULL, 10, 1, 2), (40, 1794700100, NULL, 11, 1, 1), (41, 1794700100, NULL, 11, 1, 2),
  (42, 1794800000, NULL, 12, 1, 1), (43, 1794800000, NULL, 12, 1, 2), (44, 1794800000, NULL, 12, 1, 3);
-- A retired track keeps its record and says what it was, as `REMEMBER` writes it.
INSERT INTO tracks (id, born, retired_at, record, disc, number, was_rel, was_ms, was_title, was_artist, was_album) VALUES
  (25, 1760000500, 1794500000, 6, 1, 1, 'Glass Harbour/Partial Shore (2011)/01 Sandbar.flac', 201000, 'Sandbar', 'Glass Harbour', 'Partial Shore'),
  (26, 1760000500, 1794500000, 6, 1, 2, 'Glass Harbour/Partial Shore (2011)/02 Groyne.flac', 187000, 'Groyne', 'Glass Harbour', 'Partial Shore'),
  (27, 1760000500, 1794500000, 6, 1, 5, 'Glass Harbour/Partial Shore (2011)/05 Breakwater.flac', 243000, 'Breakwater', 'Glass Harbour', 'Partial Shore');
-- The library's one folder, and the files of some records with what their tags say at MusicBrainz: Undated's one file
-- names Tide Tables' release (a tag copied in error); a file of Late Train OST gone a while names one of its own.
INSERT INTO folders (id, path, home) VALUES (0, '/music', 0);
INSERT INTO files (folder, rel, size, mtime, state, format, bitrate, duration_ms, track, bits, rate, came) VALUES
  (0, 'Glass Harbour/Tide Tables (2012)/01 Slack Water.flac', 24000000, 1760000000, 1, 'FLAC', 900, 180000, 1, 16, 44100, 1760000000),
  (0, 'Glass Harbour/Tide Tables (2012)/02 Neap.flac', 24001000, 1760000000, 1, 'FLAC', 900, 187000, 2, 16, 44100, 1760000000),
  (0, 'Glass Harbour/Tide Tables (2012)/03 Spring Tide.flac', 24002000, 1760000000, 1, 'FLAC', 900, 194000, 3, 16, 44100, 1760000000),
  (0, 'Glass Harbour/Tide Tables (2012)/04 Ebb.flac', 24003000, 1760000000, 1, 'FLAC', 900, 201000, 4, 16, 44100, 1760000000),
  (0, 'Glass Harbour/Tide Tables (2012)/05 Flood.flac', 24004000, 1760000000, 1, 'FLAC', 900, 208000, 5, 16, 44100, 1760000000),
  (0, 'Glass Harbour/Tide Tables (2012)/06 Low Water.flac', 24005000, 1760000000, 1, 'FLAC', 900, 215000, 6, 16, 44100, 1760000000),
  (0, 'Noor Field/Two Rooms (2019)/1-01 Hall.flac', 24000000, 1760000000, 1, 'FLAC', 900, 180000, 7, 16, 44100, 1760000000),
  (0, 'Noor Field/Two Rooms (2019)/1-02 Stair.flac', 24001000, 1760000000, 1, 'FLAC', 900, 187000, 8, 16, 44100, 1760000000),
  (0, 'Noor Field/Two Rooms (2019)/1-03 Door.flac', 24002000, 1760000000, 1, 'FLAC', 900, 194000, 9, 16, 44100, 1760000000),
  (0, 'Noor Field/Two Rooms (2019)/2-01 Window.flac', 24003000, 1760000000, 1, 'FLAC', 900, 201000, 10, 16, 44100, 1760000000),
  (0, 'Noor Field/Two Rooms (2019)/2-02 Floor.flac', 24004000, 1760000000, 1, 'FLAC', 900, 208000, 11, 16, 44100, 1760000000),
  (0, 'Noor Field/Two Rooms (2019)/2-03 Roof.flac', 24005000, 1760000000, 1, 'FLAC', 900, 215000, 12, 16, 44100, 1760000000),
  (0, 'Kite Theory/Night Walks EP (2020)/01 Lamplight.mp3', 24000000, 1760000000, 1, 'MP3', 320, 180000, 13, NULL, 44100, 1760000000),
  (0, 'Kite Theory/Night Walks EP (2020)/02 Crossing.mp3', 24001000, 1760000000, 1, 'MP3', 320, 187000, 14, NULL, 44100, 1760000000),
  (0, 'Kite Theory/Night Walks EP (2020)/03 Home.mp3', 24002000, 1760000000, 1, 'MP3', 320, 194000, 15, NULL, 44100, 1760000000),
  (0, 'Kite Theory/Undated (no year)/01 Morning.flac', 24000000, 1760000000, 1, 'FLAC', 900, 180000, 16, 16, 44100, 1760000000),
  (0, 'Noor Field/Signal (2021)/01 Signal.flac', 24000000, 1760000000, 1, 'FLAC', 900, 180000, 28, 16, 44100, 1760000000);
INSERT INTO files (folder, rel, size, mtime, state, format, bitrate, duration_ms, track, gone_at, came) VALUES
  (0, 'Various Artists/Late Train OST (2005)/01 Platform.flac', 25000000, 1760000000, 1, 'FLAC', 900, 200000, 20, 1794000000, 1760000000);
INSERT INTO tags (folder, rel, n, raw, name, value) VALUES
  (0, 'Glass Harbour/Tide Tables (2012)/01 Slack Water.flac', 0, 'MUSICBRAINZ_ALBUMID', 'musicbrainz_albumid', '00000000-0000-4000-8000-000000000001'),
  (0, 'Glass Harbour/Tide Tables (2012)/01 Slack Water.flac', 1, 'MUSICBRAINZ_RELEASEGROUPID', 'musicbrainz_releasegroupid', '00000000-0000-4000-8000-000000000101'),
  (0, 'Glass Harbour/Tide Tables (2012)/02 Neap.flac', 0, 'MUSICBRAINZ_ALBUMID', 'musicbrainz_albumid', '00000000-0000-4000-8000-000000000001'),
  (0, 'Glass Harbour/Tide Tables (2012)/02 Neap.flac', 1, 'MUSICBRAINZ_RELEASEGROUPID', 'musicbrainz_releasegroupid', '00000000-0000-4000-8000-000000000101'),
  (0, 'Glass Harbour/Tide Tables (2012)/03 Spring Tide.flac', 0, 'MUSICBRAINZ_ALBUMID', 'musicbrainz_albumid', '00000000-0000-4000-8000-000000000001'),
  (0, 'Glass Harbour/Tide Tables (2012)/03 Spring Tide.flac', 1, 'MUSICBRAINZ_RELEASEGROUPID', 'musicbrainz_releasegroupid', '00000000-0000-4000-8000-000000000101'),
  (0, 'Glass Harbour/Tide Tables (2012)/04 Ebb.flac', 0, 'MUSICBRAINZ_ALBUMID', 'musicbrainz_albumid', '00000000-0000-4000-8000-000000000001'),
  (0, 'Glass Harbour/Tide Tables (2012)/04 Ebb.flac', 1, 'MUSICBRAINZ_RELEASEGROUPID', 'musicbrainz_releasegroupid', '00000000-0000-4000-8000-000000000101'),
  (0, 'Glass Harbour/Tide Tables (2012)/05 Flood.flac', 0, 'MUSICBRAINZ_ALBUMID', 'musicbrainz_albumid', '00000000-0000-4000-8000-000000000001'),
  (0, 'Glass Harbour/Tide Tables (2012)/05 Flood.flac', 1, 'MUSICBRAINZ_RELEASEGROUPID', 'musicbrainz_releasegroupid', '00000000-0000-4000-8000-000000000101'),
  (0, 'Glass Harbour/Tide Tables (2012)/06 Low Water.flac', 0, 'MUSICBRAINZ_ALBUMID', 'musicbrainz_albumid', '00000000-0000-4000-8000-000000000001'),
  (0, 'Glass Harbour/Tide Tables (2012)/06 Low Water.flac', 1, 'MUSICBRAINZ_RELEASEGROUPID', 'musicbrainz_releasegroupid', '00000000-0000-4000-8000-000000000101'),
  (0, 'Noor Field/Two Rooms (2019)/1-01 Hall.flac', 0, 'MUSICBRAINZ_ALBUMID', 'musicbrainz_albumid', '00000000-0000-4000-8000-000000000002'),
  (0, 'Noor Field/Two Rooms (2019)/1-01 Hall.flac', 1, 'MUSICBRAINZ_RELEASEGROUPID', 'musicbrainz_releasegroupid', '00000000-0000-4000-8000-000000000102'),
  (0, 'Noor Field/Two Rooms (2019)/1-02 Stair.flac', 0, 'MUSICBRAINZ_ALBUMID', 'musicbrainz_albumid', '00000000-0000-4000-8000-000000000002'),
  (0, 'Noor Field/Two Rooms (2019)/1-02 Stair.flac', 1, 'MUSICBRAINZ_RELEASEGROUPID', 'musicbrainz_releasegroupid', '00000000-0000-4000-8000-000000000102'),
  (0, 'Noor Field/Two Rooms (2019)/1-03 Door.flac', 0, 'MUSICBRAINZ_ALBUMID', 'musicbrainz_albumid', '00000000-0000-4000-8000-000000000002'),
  (0, 'Noor Field/Two Rooms (2019)/1-03 Door.flac', 1, 'MUSICBRAINZ_RELEASEGROUPID', 'musicbrainz_releasegroupid', '00000000-0000-4000-8000-000000000102'),
  (0, 'Noor Field/Two Rooms (2019)/2-01 Window.flac', 0, 'MUSICBRAINZ_ALBUMID', 'musicbrainz_albumid', '00000000-0000-4000-8000-000000000002'),
  (0, 'Noor Field/Two Rooms (2019)/2-01 Window.flac', 1, 'MUSICBRAINZ_RELEASEGROUPID', 'musicbrainz_releasegroupid', '00000000-0000-4000-8000-000000000102'),
  (0, 'Noor Field/Two Rooms (2019)/2-02 Floor.flac', 0, 'MUSICBRAINZ_ALBUMID', 'musicbrainz_albumid', '00000000-0000-4000-8000-000000000002'),
  (0, 'Noor Field/Two Rooms (2019)/2-02 Floor.flac', 1, 'MUSICBRAINZ_RELEASEGROUPID', 'musicbrainz_releasegroupid', '00000000-0000-4000-8000-000000000102'),
  (0, 'Noor Field/Two Rooms (2019)/2-03 Roof.flac', 0, 'MUSICBRAINZ_ALBUMID', 'musicbrainz_albumid', '00000000-0000-4000-8000-000000000002'),
  (0, 'Noor Field/Two Rooms (2019)/2-03 Roof.flac', 1, 'MUSICBRAINZ_RELEASEGROUPID', 'musicbrainz_releasegroupid', '00000000-0000-4000-8000-000000000102'),
  (0, 'Kite Theory/Night Walks EP (2020)/01 Lamplight.mp3', 0, 'MUSICBRAINZ_ALBUMID', 'musicbrainz_albumid', '00000000-0000-4000-8000-000000000003'),
  (0, 'Kite Theory/Night Walks EP (2020)/01 Lamplight.mp3', 1, 'MUSICBRAINZ_RELEASEGROUPID', 'musicbrainz_releasegroupid', '00000000-0000-4000-8000-000000000103'),
  (0, 'Kite Theory/Night Walks EP (2020)/02 Crossing.mp3', 0, 'MUSICBRAINZ_ALBUMID', 'musicbrainz_albumid', '00000000-0000-4000-8000-000000000003'),
  (0, 'Kite Theory/Night Walks EP (2020)/02 Crossing.mp3', 1, 'MUSICBRAINZ_RELEASEGROUPID', 'musicbrainz_releasegroupid', '00000000-0000-4000-8000-000000000103'),
  (0, 'Kite Theory/Night Walks EP (2020)/03 Home.mp3', 0, 'MUSICBRAINZ_ALBUMID', 'musicbrainz_albumid', '00000000-0000-4000-8000-000000000003'),
  (0, 'Kite Theory/Night Walks EP (2020)/03 Home.mp3', 1, 'MUSICBRAINZ_RELEASEGROUPID', 'musicbrainz_releasegroupid', '00000000-0000-4000-8000-000000000103'),
  (0, 'Kite Theory/Undated (no year)/01 Morning.flac', 0, 'MUSICBRAINZ_ALBUMID', 'musicbrainz_albumid', '00000000-0000-4000-8000-000000000001'),
  (0, 'Noor Field/Signal (2021)/01 Signal.flac', 0, 'MUSICBRAINZ_ALBUMID', 'musicbrainz_albumid', '00000000-0000-4000-8000-000000000007'),
  (0, 'Noor Field/Signal (2021)/01 Signal.flac', 1, 'MUSICBRAINZ_RELEASEGROUPID', 'musicbrainz_releasegroupid', '00000000-0000-4000-8000-000000000107'),
  (0, 'Various Artists/Late Train OST (2005)/01 Platform.flac', 0, 'MUSICBRAINZ_ALBUMID', 'musicbrainz_albumid', '00000000-0000-4000-8000-000000000005');
-- Partial Shore came back after it retired (record 8): its files name the release the retired record was linked to, its first
-- file's tag the odd one out.
INSERT INTO files (folder, rel, size, mtime, state, format, bitrate, duration_ms, track, bits, rate, came) VALUES
  (0, 'Glass Harbour/Partial Shore (2011)/01 Sandbar.flac', 26000000, 1794600000, 1, 'FLAC', 900, 201000, 35, 16, 44100, 1794600000),
  (0, 'Glass Harbour/Partial Shore (2011)/02 Groyne.flac', 26000001, 1794600000, 1, 'FLAC', 900, 202000, 36, 16, 44100, 1794600000),
  (0, 'Glass Harbour/Partial Shore (2011)/05 Breakwater.flac', 26000002, 1794600000, 1, 'FLAC', 900, 203000, 37, 16, 44100, 1794600000);
INSERT INTO tags (folder, rel, n, raw, name, value) VALUES
  (0, 'Glass Harbour/Partial Shore (2011)/01 Sandbar.flac', 0, 'MUSICBRAINZ_ALBUMID', 'musicbrainz_albumid', '00000000-0000-4000-8000-000000000060'),
  (0, 'Glass Harbour/Partial Shore (2011)/02 Groyne.flac', 0, 'MUSICBRAINZ_ALBUMID', 'musicbrainz_albumid', '00000000-0000-4000-8000-000000000006'),
  (0, 'Glass Harbour/Partial Shore (2011)/05 Breakwater.flac', 0, 'MUSICBRAINZ_ALBUMID', 'musicbrainz_albumid', '00000000-0000-4000-8000-000000000006');
-- Tide Tables' files name their album artist and MusicBrainz's id for them: how the step names a discography's artist.
INSERT INTO tags (folder, rel, n, raw, name, value) VALUES
  (0, 'Glass Harbour/Tide Tables (2012)/01 Slack Water.flac', 2, 'ALBUMARTIST', 'albumartist', 'Glass Harbour'),
  (0, 'Glass Harbour/Tide Tables (2012)/01 Slack Water.flac', 3, 'MUSICBRAINZ_ALBUMARTISTID', 'musicbrainz_albumartistid', '00000000-0000-4000-8000-000000000201'),
  (0, 'Glass Harbour/Tide Tables (2012)/02 Neap.flac', 2, 'ALBUMARTIST', 'albumartist', 'Glass Harbour'),
  (0, 'Glass Harbour/Tide Tables (2012)/02 Neap.flac', 3, 'MUSICBRAINZ_ALBUMARTISTID', 'musicbrainz_albumartistid', '00000000-0000-4000-8000-000000000201'),
  (0, 'Glass Harbour/Tide Tables (2012)/03 Spring Tide.flac', 2, 'ALBUMARTIST', 'albumartist', 'Glass Harbour'),
  (0, 'Glass Harbour/Tide Tables (2012)/03 Spring Tide.flac', 3, 'MUSICBRAINZ_ALBUMARTISTID', 'musicbrainz_albumartistid', '00000000-0000-4000-8000-000000000201'),
  (0, 'Glass Harbour/Tide Tables (2012)/04 Ebb.flac', 2, 'ALBUMARTIST', 'albumartist', 'Glass Harbour'),
  (0, 'Glass Harbour/Tide Tables (2012)/04 Ebb.flac', 3, 'MUSICBRAINZ_ALBUMARTISTID', 'musicbrainz_albumartistid', '00000000-0000-4000-8000-000000000201'),
  (0, 'Glass Harbour/Tide Tables (2012)/05 Flood.flac', 2, 'ALBUMARTIST', 'albumartist', 'Glass Harbour'),
  (0, 'Glass Harbour/Tide Tables (2012)/05 Flood.flac', 3, 'MUSICBRAINZ_ALBUMARTISTID', 'musicbrainz_albumartistid', '00000000-0000-4000-8000-000000000201'),
  (0, 'Glass Harbour/Tide Tables (2012)/06 Low Water.flac', 2, 'ALBUMARTIST', 'albumartist', 'Glass Harbour'),
  (0, 'Glass Harbour/Tide Tables (2012)/06 Low Water.flac', 3, 'MUSICBRAINZ_ALBUMARTISTID', 'musicbrainz_albumartistid', '00000000-0000-4000-8000-000000000201');
-- Every file names its album and its album artist, as 0.16's first start named each record by them.
INSERT INTO tags (folder, rel, n, raw, name, value)
  SELECT f.folder, f.rel, 10, 'ALBUM', 'album', r.title FROM files f JOIN tracks t ON t.id = f.track JOIN records r ON r.id = t.record;
INSERT INTO tags (folder, rel, n, raw, name, value)
  SELECT f.folder, f.rel, 11, 'ALBUMARTIST', 'albumartist', r.artist FROM files f JOIN tracks t ON t.id = f.track JOIN records r ON r.id = t.record
  WHERE NOT EXISTS (SELECT 1 FROM tags g WHERE g.folder = f.folder AND g.rel = f.rel AND g.name = 'albumartist');
-- 0.16's first start, done.
INSERT INTO meta (key, value) VALUES
  ('first_records', '{"records":6,"copies":6,"tracks_merged":0,"bases_on_two_records":0,"artists":6,"shapes_moved":0,"shapes_left":0,"beats_moved":0,"beats_left":0}'),
  ('came', 'filled'), ('tags', 'named');
