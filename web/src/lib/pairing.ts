// Pairing (QR04, QR07; QR01, QR02 and QR06 are the server's): an app paired from Account by its own page, the QR and the code; a
// browser signed in by another device's Allow, the browser asking (the owner, 2026-11-17: "Browsers too, they ask"), each asking
// the account's password first (OQ-B2 (a)). What the pages decide, pure, so the tests take it as it is.

/** The codes' alphabet (QR01): 32 signs, none of 0, O, 1 or I. */
export const PAIR_ALPHABET = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789';
/** A code's length. */
export const CODE_LENGTH = 6;

/** A code as typed (QR07.5): shown in upper case as typed, spaces left out; `code` once it is one, else null (fewer than six
 *  signs, or a sign outside the alphabet: nothing is sent for it). */
export function typedCode(text: string): { shown: string; code: string | null } {
  const shown = text.toUpperCase().replace(/\s+/g, '');
  const code = shown.length === CODE_LENGTH && [...shown].every((c) => PAIR_ALPHABET.includes(c)) ? shown : null;
  return { shown, code };
}

/** What `GET /api/pair` says of the account's code (QR02.4): waiting, paired (with the device), or none (used, replaced, expired). */
export type PairState = { state: 'waiting' | 'paired' | 'none'; device?: string | null };

/** The Pair page (QR04): it shows the QR and the code while the code waits, and steps back to the list once paired, or once the
 *  code is gone, where the new sign-in shows (or none did). Not read yet: it waits as it is. */
export const pairStep = (s: PairState | null): 'show' | 'back' => (s && s.state !== 'waiting' ? 'back' : 'show');

/** What asked (`GET /api/pair/allow/{code}`, QR06.3; OQ-B1 (a)): the device from the asking browser's word, when it asked (epoch
 *  seconds), and whether from home, which the asker cannot choose. */
export type PairAsk = { device: string; asked: number; home: boolean };

/** How long ago, in a few words: "just now", "2 min ago", "3 h ago". */
export function ago(at: number, now: number): string {
  const s = Math.max(0, Math.round(now - at));
  if (s < 60) return 'just now';
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  return `${Math.floor(s / 3600)} h ago`;
}

/** The Allow sheet's device line (QR07.1): "Firefox on Windows · 2 min ago · away from home". `now` in epoch seconds. */
export const askLine = (a: PairAsk, now: number) => `${a.device} · ${ago(a.asked, now)} · ${a.home ? 'at home' : 'away from home'}`;

/** A code that does not work (QR07.4): one line, whatever the cause (unknown, used, refused, expired, the server restarted). */
export const DEAD_CODE = "That code didn't work.";

/** The page's address that opens the Allow sheet (QR06.9: `/pair/allow?code=…`, the QR's link): its code, read as typed; null
 *  for any other address, or a code that cannot be one. */
export function allowCodeIn(pathname: string, search: string): string | null {
  if (pathname.replace(/\/+$/, '') !== '/pair/allow') return null;
  return typedCode(new URLSearchParams(search).get('code') ?? '').code;
}

/** What the Allow sheet shows (QR07): the code read, or not yet; one line for a dead code; out of reach, the line every page says
 *  then and no Allow (QR07.6); else what asked, with Allow and Refuse. */
export type AllowShows = 'reading' | 'dead' | 'away' | 'ask';
export function allowShows(read: PairAsk | 'dead' | null, away: boolean): AllowShows {
  if (away) return 'away';
  if (read === null) return 'reading';
  return read === 'dead' ? 'dead' : 'ask';
}
