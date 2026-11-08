-- The sync's agreed lists as 0.15 left them: catalogue track ids, the third one's on a copy the first start merges.
CREATE TABLE links (source TEXT NOT NULL, user TEXT NOT NULL, name TEXT NOT NULL, remote TEXT NOT NULL, remote_name TEXT NOT NULL, stamp TEXT, base TEXT NOT NULL, base_ids TEXT NOT NULL DEFAULT '[]', PRIMARY KEY (source, user, name)) WITHOUT ROWID;
INSERT INTO links (source, user, name, remote, remote_name, stamp, base, base_ids) VALUES ('jf', 'u1', 'Mix', 'r1', 'Mix', 's1', '[83,19,87]', '["a","b","c"]');
