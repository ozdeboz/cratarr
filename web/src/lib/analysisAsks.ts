// The page's asks for a track's analysis (waveform, spectrum, beats, signals): one at a time, the most wanted first.
// The server works each out in turn behind its HEAVY gate and answers 503 after 20 s waiting there; asked all at once
// (the playing track's four, the next one's two) the later ones only waited, timed out and were asked again
// (2026-10-26: 13 in three minutes on the test server, a seek's stream waiting behind them).

/** How wanted an ask is: the playing track's waveform, then its other analysis, then the next track's. */
export const WAVE = 0;
export const PLAYING = 1;
export const NEXT = 2;

type Ask = { url: string; rank: number; go: () => void };
const waiting: Ask[] = [];
let busy = false;

function pump() {
  if (busy) return;
  waiting.sort((a, b) => a.rank - b.rank);
  const ask = waiting.shift();
  if (!ask) return;
  busy = true;
  ask.go();
}

/** Fetches `url` once the asks before it are answered (their headers in: the server's work done). */
export function askAnalysis(url: string, rank: number): Promise<Response> {
  return new Promise((resolve, reject) => {
    waiting.push({
      url,
      rank,
      go: () =>
        fetch(url)
          .then(resolve, reject)
          .finally(() => {
            busy = false;
            pump();
          }),
    });
    pump();
  });
}

/** The server busy (its HEAVY turn not reached): asked again later, not taken as none. */
export const BUSY = 503;
/** How long before a busy ask is asked again, and how often at most. */
export const AGAIN_MS = 15_000;
export const TRIES = 3;
