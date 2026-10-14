// Records the first channel on the audio thread, in batches of 8192 frames (no main-thread drops).
class Capture extends AudioWorkletProcessor {
  constructor() {
    super();
    this.buf = new Float32Array(8192);
    this.n = 0;
    this.on = false;
    this.port.onmessage = (e) => {
      this.on = e.data === 'start';
      if (!this.on && this.n) {
        this.port.postMessage(this.buf.slice(0, this.n));
        this.n = 0;
      }
    };
  }
  process(inputs, outputs) {
    const ch = inputs[0] && inputs[0][0];
    if (this.on && ch) {
      for (let i = 0; i < ch.length; i++) {
        this.buf[this.n++] = ch[i];
        if (this.n === this.buf.length) {
          this.port.postMessage(this.buf.slice());
          this.n = 0;
        }
      }
    }
    const out = outputs[0] && outputs[0][0];
    if (out) out.fill(0); // silent: nothing reaches the speakers
    return true;
  }
}
registerProcessor('capture', Capture);
