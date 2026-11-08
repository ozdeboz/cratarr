// Covers that failed to load, kept so every place that shows one shows the stand-in at once without asking again: a record
// with no cover answers 404 at every size. But an image's failure does not say why, and one asked while Cratarr was out of
// reach failed for the network, not for the record (0.16.0 on the tablet, 2026-11-08: every cover asked with Wi-Fi off stayed
// a stand-in once it was back, until the page was opened again). So the failures noted while the live channel was down, and
// in the moments before it was seen to drop, are forgotten when it comes back, and those covers are asked again.
import { stream } from './events.svelte';

/** How long before the channel is seen to drop a failure may still be the network's: a request in flight fails first. */
export const GRACE_MS = 30_000;

export type Failures = {
  /** The covers that failed, by their address: read it to show the stand-in. */
  readonly failed: Record<string, true>;
  note(src: string): void;
};

const all: { failed: Record<string, true>; at: Map<string, number> }[] = [];
let downSince: number | null = null;

/** A set of failures of its own, forgotten with every other's when the channel comes back. */
export function failures(): Failures {
  const failed = $state<Record<string, true>>({});
  const at = new Map<string, number>();
  all.push({ failed, at });
  return {
    failed,
    note(src: string) {
      failed[src] = true;
      at.set(src, Date.now());
    },
  };
}

/** The channel came back after being down since `since`: what failed from a little before then on is asked again. */
export function forgetSince(since: number) {
  const from = since - GRACE_MS;
  for (const { failed, at } of all) {
    for (const [src, when] of at) {
      if (when < from) continue;
      delete failed[src];
      at.delete(src);
    }
  }
}

/** The channel as it is now: `connected` false notes when it went, true forgets what failed since. */
export function channel(connected: boolean, now = Date.now()) {
  if (!connected) {
    downSince ??= now;
  } else if (downSince !== null) {
    forgetSince(downSince);
    downSince = null;
  }
}

$effect.root(() => {
  $effect(() => channel(stream.connected));
});
