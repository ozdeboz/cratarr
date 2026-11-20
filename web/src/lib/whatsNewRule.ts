// What's New (UC03; UC02 is the server's): the notes an account has not seen, as the server reads them (UC-Q2 (a): the running
// version's bundled, a newer one's from its release, each parsed into parts, a heading and its lines), and who sees what (UC-Q3 (a):
// administrators that a newer release is out; everyone the running release's notes once after an upgrade; a fresh account only
// the newest). Pure, so the tests take it as it is.

/** One version's notes: its parts, each a heading (none for its opening lines) and its lines; `newer`: a release newer than the
 *  one running, which only an administrator is told of. */
export type WhatsNewNote = { version: string; newer: boolean; parts: { heading: string | null; lines: string[] }[] };
/** What `GET /api/whats-new` answers: whether there is something this account has not seen, and the notes, newest first. */
export type WhatsNew = { unread: boolean; notes: WhatsNewNote[] };

/** The dot on Settings' info pill and in About: something not seen, with notes to show. */
export const whatsNewDot = (w: WhatsNew | null | undefined): boolean => !!w && w.unread && w.notes.length > 0;

/** The version marked seen when the notes are opened: the newest shown. */
export const newestOf = (w: WhatsNew | null | undefined): string | null => w?.notes[0]?.version ?? null;

/** A note's heading: its version, said newer where it is not the one running. */
export const noteTitle = (n: WhatsNewNote) => (n.newer ? `${n.version} · newer` : n.version);
