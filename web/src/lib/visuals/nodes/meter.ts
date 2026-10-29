// The Meter (decision 0057 §3): a detector on the picture that feeds the Screen. That picture is read back small, in
// tiles (flashmeter.ts: two or more frames late, never stalling); with the frame's own numbers (the parts' turning,
// shapes and colours, the looks) meters.ts measures from them what the frame did: its flashes, motion, churn, zoom
// pumping, looks a minute. Its values (MeterValues) are what the engine's Meter module gives the next frame (the
// Compressors' keys, the Screen's flash guard).
import { FlashMeter } from '../flashmeter';
import { Meters, type FrameStats, type MeterValues, type Tiles } from '../meters';

export class Meter {
  private gl: WebGL2RenderingContext;
  // Made the first time the picture is measured; out where it cannot be made.
  private flash: FlashMeter | null = null;
  private out = false;
  private meters = new Meters();
  /** The readings in since the frame's numbers were last given. */
  private unread: Tiles[] = [];

  constructor(gl: WebGL2RenderingContext) {
    this.gl = gl;
  }

  /** The screen just shown (`viewWidth` x `viewHeight`) read into tiles, as of `t` seconds. None where this device
   *  cannot (it is then never tried again). */
  capture(viewWidth: number, viewHeight: number, t: number) {
    if (this.out) return;
    try {
      this.flash ??= new FlashMeter(this.gl);
      this.flash.capture(viewWidth, viewHeight, t);
    } catch (e) {
      console.warn('Visuals: no flash meter on this device', e);
      this.flash?.dispose();
      this.flash = null;
      this.out = true;
    }
  }

  /** The readings the GPU has finished, the oldest first (kept for the frame's numbers too). Once a frame, before
   *  anything draws (flashmeter.ts). */
  read(): Tiles[] {
    const got = this.flash?.poll() ?? [];
    this.unread.push(...got);
    return got;
  }

  /** The frame's own numbers, with the readings in since. */
  add(stats: Omit<FrameStats, 'tiles'>) {
    this.meters.add({ ...stats, tiles: this.unread.splice(0) });
  }

  /** What the frames measure now. */
  values(): MeterValues {
    return this.meters.now();
  }

  /** Measuring anew (a trial of the rating page starts: one's flashes and motion are not counted in the next). */
  reset() {
    this.meters = new Meters();
  }

  /** The readings let go unread, the readback's ring full. */
  get dropped(): number {
    return this.flash?.dropped ?? 0;
  }

  dispose() {
    this.flash?.dispose();
    this.flash = null;
  }
}
