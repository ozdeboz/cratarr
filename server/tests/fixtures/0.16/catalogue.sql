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
-- `counting`. Records 1 to 5 are had; 6 is retired, its tracks retired with it; 7 came after the first start.
INSERT INTO records (id, born, retired_at, title, artist, year, kind, counting) VALUES
  (1, 1760000000, NULL, 'Tide Tables', 'Glass Harbour', 2012, NULL, NULL),
  (2, 1760000100, NULL, 'Two Rooms', 'Noor Field', 2019, NULL, NULL),
  (3, 1760000200, NULL, 'Night Walks EP', 'Kite Theory', 2020, NULL, NULL),
  (4, 1760000300, NULL, 'Undated', 'Kite Theory', NULL, NULL, NULL),
  (5, 1760000400, NULL, 'Late Train OST', 'Various Artists', 2005, NULL, NULL),
  (6, 1760000500, 1794500000, 'Partial Shore', 'Glass Harbour', 2011, NULL, NULL),
  (7, 1794200000, NULL, 'Signal', 'Noor Field', 2021, NULL, NULL);
INSERT INTO tracks (id, born, retired_at, record, disc, number) VALUES
  (1, 1760000000, NULL, 1, 1, 1), (2, 1760000000, NULL, 1, 1, 2), (3, 1760000000, NULL, 1, 1, 3),
  (4, 1760000000, NULL, 1, 1, 4), (5, 1760000000, NULL, 1, 1, 5), (6, 1760000000, NULL, 1, 1, 6),
  (7, 1760000100, NULL, 2, 1, 1), (8, 1760000100, NULL, 2, 1, 2), (9, 1760000100, NULL, 2, 1, 3),
  (10, 1760000100, NULL, 2, 2, 1), (11, 1760000100, NULL, 2, 2, 2), (12, 1760000100, NULL, 2, 2, 3),
  (13, 1760000200, NULL, 3, 1, 1), (14, 1760000200, NULL, 3, 1, 2), (15, 1760000200, NULL, 3, 1, 3),
  (16, 1760000300, NULL, 4, 1, 1), (17, 1760000300, NULL, 4, 1, 2), (18, 1760000300, NULL, 4, 1, 3), (19, 1760000300, NULL, 4, 1, 4),
  (20, 1760000400, NULL, 5, 1, 1), (21, 1760000400, NULL, 5, 1, 2), (22, 1760000400, NULL, 5, 1, 3), (23, 1760000400, NULL, 5, 1, 4), (24, 1760000400, NULL, 5, 1, 5),
  (28, 1794200000, NULL, 7, 1, 1);
-- A retired track keeps its record and says what it was, as `REMEMBER` writes it.
INSERT INTO tracks (id, born, retired_at, record, disc, number, was_rel, was_ms, was_title, was_artist, was_album) VALUES
  (25, 1760000500, 1794500000, 6, 1, 1, 'Glass Harbour/Partial Shore (2011)/01 Sandbar.flac', 201000, 'Sandbar', 'Glass Harbour', 'Partial Shore'),
  (26, 1760000500, 1794500000, 6, 1, 2, 'Glass Harbour/Partial Shore (2011)/02 Groyne.flac', 187000, 'Groyne', 'Glass Harbour', 'Partial Shore'),
  (27, 1760000500, 1794500000, 6, 1, 5, 'Glass Harbour/Partial Shore (2011)/05 Breakwater.flac', 243000, 'Breakwater', 'Glass Harbour', 'Partial Shore');
-- 0.16's first start, done.
INSERT INTO meta (key, value) VALUES
  ('first_records', '{"records":6,"copies":6,"tracks_merged":0,"bases_on_two_records":0,"artists":6,"shapes_moved":0,"shapes_left":0,"beats_moved":0,"beats_left":0}'),
  ('came', 'filled'), ('tags', 'named');
