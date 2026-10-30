// The speeds a browser plays a media element at with its sound (the owner, 2026-10-28): Chromium's and WebKit's from
// 1/16 to 16x; Gecko's (Firefox on a computer or Android; not on iOS, which is WebKit inside) only from 0.25 to 4x, muted
// outside. Every brake's and spin-up's slowest, a hold's top and the speed a hold is handed back at read these.
const ua = typeof navigator === 'undefined' ? '' : navigator.userAgent;
const gecko = /\bFirefox\/\d/.test(ua) && !/\bFxiOS\//.test(ua);
/** The slowest it plays with sound (×). */
export const RATE_FLOOR = gecko ? 0.25 : 1 / 16;
/** The fastest it plays with sound (×). */
export const RATE_TOP = gecko ? 4 : 16;
