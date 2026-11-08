// Small covers as they come off the wire (D13, the server's `app/device_covers.rs`), pure: `CRXV`, its version and its rev, then for each
// record its id, its picture's length and the picture, little-endian, read to the end of the body. A bundle can be tens of MB, so
// nothing here waits for the whole of it. What the head says is the rule's to read (`index::sync`): this only finds where things end.

/** How much of them says what they are (`COVERS_HEAD`). */
const HEAD = 16;
/** A frame before its picture: the record, the picture's length. */
const FRAME = 12;

/** A record's small cover; `jpeg` null: it has none now. */
export type CoverFrame = { record: number; jpeg: Uint8Array | null };

export class CoverFrames {
  private held = new Uint8Array(0);
  private headGiven = false;

  /** More of the body, as it came. */
  push(chunk: Uint8Array) {
    const joined = new Uint8Array(this.held.length + chunk.length);
    joined.set(this.held);
    joined.set(chunk, this.held.length);
    this.held = joined;
  }

  /** The head, once it is all here; null before, and once it has been given. */
  head(): Uint8Array | null {
    if (this.headGiven || this.held.length < HEAD) return null;
    this.headGiven = true;
    const head = this.held.slice(0, HEAD);
    this.held = this.held.slice(HEAD);
    return head;
  }

  /** The frames here whole, each given once. */
  take(): CoverFrame[] {
    const frames: CoverFrame[] = [];
    const view = new DataView(this.held.buffer, this.held.byteOffset, this.held.byteLength);
    let at = 0;
    while (this.held.length - at >= FRAME) {
      const length = view.getUint32(at + 8, true);
      if (this.held.length - at - FRAME < length) break;
      const jpeg = length === 0 ? null : this.held.slice(at + FRAME, at + FRAME + length);
      frames.push({ record: Number(view.getBigInt64(at, true)), jpeg });
      at += FRAME + length;
    }
    this.held = this.held.slice(at);
    return frames;
  }

  /** Whether something unfinished is left: at the body's end, it was cut short. */
  get cut() {
    return this.held.length > 0;
  }
}
