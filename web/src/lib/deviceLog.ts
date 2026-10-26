// This page's own log (decision 0045, item 2): numbered lines, the newest 500 kept, sent to Cratarr's diagnostics log
// under this page's player (lib/playOnTarget ships them while it is connected). `playon`: what became of its connection;
// `playback` (the owner, 2026-10-26: "no errors when it breaks, so we need better instrumentation"): what the browser's
// player did with each track, every refusal and stall included, so a track that will not play on one browser (Safari on
// iOS) says why.
export type LogLine = { seq: number; at: number; level: 'info' | 'warn'; area: 'playon' | 'playback'; what: string; fields: Record<string, unknown> };
export const logLines: LogLine[] = [];
let seq = 0;
/** The newest line's number. */
export const lastSeq = () => seq;
export function note(level: LogLine['level'], what: string, fields: Record<string, unknown> = {}, area: LogLine['area'] = 'playon') {
  logLines.push({ seq: ++seq, at: Date.now(), level, area, what, fields });
  if (logLines.length > 500) logLines.shift();
  if (level === 'warn') soon?.();
}
/** A warning goes to Cratarr within moments, not with the next round (set by lib/playOnTarget). */
let soon: (() => void) | null = null;
export const sendSoonWith = (send: () => void) => (soon = send);
/** A player's line, its fields kept short. */
export const playback = (level: LogLine['level'], what: string, fields: Record<string, unknown> = {}) => note(level, what, fields, 'playback');
