// Pairing's two codes as the page holds them (QR04, QR07; the rules lib/pairing): the app's, shown on Account's Pair page while it
// waits; and a browser's asking, whose Allow sheet App shows (from the QR's link, or typed in Account's Allow a device).
export const pairs = $state({
  /** This account's code for an app (QR04), from its password asked again; null once paired or gone. */
  code: null as string | null,
  /** A browser's code being allowed (QR07): its sheet shows while it is set. */
  allow: null as string | null,
});
