-- A journal as 0.15 left it: what a day's Undo reads names the ids that move.
CREATE TABLE events (id INTEGER PRIMARY KEY AUTOINCREMENT, at INTEGER NOT NULL, event TEXT NOT NULL, undone_at INTEGER, applied INTEGER NOT NULL DEFAULT 1, account TEXT, undone_by TEXT);
CREATE TABLE handled (event INTEGER NOT NULL, handler TEXT NOT NULL, at INTEGER NOT NULL, result TEXT NOT NULL, PRIMARY KEY (event, handler)) WITHOUT ROWID;
CREATE TABLE changes (id INTEGER PRIMARY KEY AUTOINCREMENT, at INTEGER NOT NULL, change TEXT NOT NULL, undone_at INTEGER, account TEXT);
CREATE TABLE file_runs (id INTEGER PRIMARY KEY AUTOINCREMENT, at INTEGER NOT NULL, run TEXT NOT NULL, account TEXT, undone_at INTEGER);
INSERT INTO changes (id, at, change, undone_at, account) VALUES (1, 1760999980, '{"kind":"left_out","crate_id":"like:al-4FwVmWLNdYpZZSEtkan6Uy","before":{"lines":[],"albums":[]},"after":{"lines":["Glass Harbour/Tide Tables (2012)/02 Neap.flac"],"albums":["al-7ATEPHL21gflZtRCnZNzGM"]}}', NULL, 'ann');
INSERT INTO changes (id, at, change, undone_at, account) VALUES (2, 1760999985, '{"kind":"excluded","before":[],"after":["Salt Meridian/Both Shores (2018)/01 Landing.flac"]}', NULL, 'ann');
