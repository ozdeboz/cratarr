// What the graph's live faces read (faces.ts): the frame loop offers each frame, kept at most ~30 times a second, so a
// face redraws no more often than that and only while something is drawn from it (the graph is shown).
import type { LiveFrame } from './faces';

export class LiveFaces {
  frame = $state.raw<LiveFrame | null>(null);
  private at = -Infinity;
  constructor(private readonly hz = 30) {}

  /** A frame at `now` (ms): `make` is called only when one is due. */
  offer(now: number, make: () => LiveFrame) {
    if (now - this.at < 1000 / this.hz - 1) return;
    this.at = now;
    this.frame = make();
  }
}
