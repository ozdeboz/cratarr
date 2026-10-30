// The server's connection as App works it out from its status (shown by the mode switch, lib/ModeSwitcher, in the bar
// or at the top of Settings): ok, and what to say; null before the first answer.
export const serverConnection = $state({ value: null as { ok: boolean; text: string } | null });
