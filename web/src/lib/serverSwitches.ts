// Two administrators' switches in Settings › Connections › Discovery and metadata (W13; run 7d's O4 and O5): Bandcamp, on or off,
// and until when it is left alone after its browser check (switching it on again lifts that, V8); and "Pause background
// reading", on while the server's background reading is off. Pure.

/** Bandcamp's setting as the server answers it (`GET /api/settings/bandcamp`): `held_until` (epoch seconds) after its check. */
export type BandcampSetting = { enabled: boolean; held_until?: number | null };
/** The background reading as the server answers it (`GET /api/settings/background`): off is paused. */
export type BackgroundSetting = { enabled: boolean };

/** Whether Bandcamp is held now: on, and its check's day not over. */
export const bandcampHeld = (s: BandcampSetting, now: number) => s.enabled && !!s.held_until && s.held_until > now;

/** Its row: its dot, and its one line (off, or until when it is held; nothing while it simply works). */
export function bandcampRow(s: BandcampSetting, now: number, at: (t: number) => string): { lead: 'on' | 'off' | 'warn'; line: string | null } {
  if (!s.enabled) return { lead: 'off', line: 'Off' };
  return bandcampHeld(s, now) ? { lead: 'warn', line: `Held until ${at(s.held_until!)}` } : { lead: 'on', line: null };
}

/** "Pause background reading" is on while the reading is off; switched, it sends the reading's own `enabled`. */
export const backgroundPaused = (s: BackgroundSetting) => !s.enabled;
export const pausedBody = (paused: boolean): BackgroundSetting => ({ enabled: !paused });
