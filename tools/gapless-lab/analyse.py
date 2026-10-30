"""analyse.py <capture.f32> <reference.f32> <boundary samples, comma-separated>
Aligns the capture to the reference and reports, through time, how far the capture is shifted against the
reference (in samples), plus silence; a jump in the shift at a boundary is samples lost (+) or repeated (-)."""
import sys, numpy as np
SR = 44100
C = np.fromfile(sys.argv[1], dtype='<f4'); R = np.fromfile(sys.argv[2], dtype='<f4')
B = [int(b) for b in sys.argv[3].split(',')] if len(sys.argv) > 3 and sys.argv[3] else []

def lag_of(cseg, rseg):
    n = 1 << int(np.ceil(np.log2(len(cseg) + len(rseg))))
    xc = np.fft.irfft(np.fft.rfft(cseg, n) * np.conj(np.fft.rfft(rseg, n)), n)
    k = int(np.argmax(xc)); k = k if k < n // 2 else k - n
    norm = xc[k] / np.sqrt(np.sum(rseg ** 2) * np.sum(cseg ** 2) + 1e-12)
    return k, float(norm)

# Global: where reference sample s appears in the capture = s + off.
seg = R[2 * SR:6 * SR]
lag, fit = lag_of(C, seg)
off = lag - 2 * SR
print(f'capture {len(C)/SR:.2f} s; reference sample s is at capture index s{off:+d} (fit {fit:.3f})')
# Through time: windows of 4096 reference samples; local shift relative to `off`.
events, prev = [], None
for s in range(SR // 4, len(R) - SR // 4, SR // 4):
    r = R[s:s + 4096]
    a = s + off - 6000
    if a < 0 or a + 4096 + 12000 > len(C): continue
    k, f = lag_of(C[a:a + 4096 + 12000], r)
    shift = k - 6000
    if prev is None or shift != prev:
        events.append(f'{s/SR:6.2f}s: shift {shift:+d} (fit {f:.3f})')
    prev = shift
print('shift through time (only changes):'); print('  ' + '\n  '.join(events))
def shift_at(s, guess):
    # The shift at reference sample s, searched around a guess (so capture losses elsewhere do not matter).
    r = R[s:s + 4096]
    a = s + off + guess - 3000
    if a < 0 or a + 4096 + 6000 > len(C): return None, 0.0
    k, f = lag_of(C[a:a + 4096 + 6000], r)
    return guess + k - 3000, f

guess = 0
for b in B:
    before, fb = shift_at(b - SR // 2, guess)
    if before is None: continue
    before2, _ = shift_at(b - 6000, before)
    after, fa = shift_at(b + 2000, before2)
    guess = after
    i = b + off + before2
    w = C[max(0, i - 3000):i + 3000]
    print(f'boundary {b/SR:.4f}s: jump {after - before2:+d} samples (0 = gapless), shift before {before2:+d}, after {after:+d}, '
          f'silent samples near it {int(np.sum(np.abs(w) < 1e-4))}')
