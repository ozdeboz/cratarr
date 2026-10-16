// Whether Cratarr answers. Away from home the proxy in front of it refuses with 403 (Cratarr itself never does); a
// stopped Cratarr gets the proxy's empty 502 to 504; without a network the request fails. All three are out of reach:
// the pages say so in one line, the pollers ask less often, and the app opens on its downloads (decision 0033, step 4).

export const reach = $state({
  /** The last request did not reach Cratarr. */
  away: false,
  /** Cratarr answered at least once since the page opened. */
  answered: false,
});

/** A status with no words from Cratarr in it that means it was not reached, rather than that it refused. */
export const notReached = (status: number) => status === 403 || status === 502 || status === 503 || status === 504;

export function reached() {
  reach.away = false;
  reach.answered = true;
}

export function unreached() {
  reach.away = true;
}
