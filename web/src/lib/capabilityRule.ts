// What the server can do, as /api/capabilities says it (0.11; decision 0085, step 1: "we don't show what we cannot
// consume, and we don't offer what we cannot uphold"): a name listed is something it can do, one not listed something
// it cannot. A server before 0.11 has no list (it answers 404): there, and until a first answer (or the last one kept)
// is known, nothing is decided by the list, and each place keeps the check it had before (a 404, a field's presence).
// Pure, so the tests take it as it is.

export type CapabilityState = {
  /** The last answer's names (kept for offline), or null when none is known. */
  names: readonly string[] | null;
  /** The server answered that it has no list: one before 0.11. */
  older: boolean;
};

export const NO_CAPABILITIES: CapabilityState = { names: null, older: false };

/** Whether the server can do `name`: true or false where its list says, null where the list decides nothing (a server
 *  before 0.11, or no answer known yet) and the place's own check stands. */
export function canBy(s: CapabilityState, name: string): boolean | null {
  if (s.older || s.names === null) return null;
  return s.names.includes(name);
}

/** The state after an answer: its names; a 404 (`older`); anything else (out of reach, a refusal) leaves what was known. */
export function answered(s: CapabilityState, a: { names: readonly string[] } | 'older' | 'failed'): CapabilityState {
  if (a === 'failed') return s;
  if (a === 'older') return { names: null, older: true };
  return { names: [...a.names], older: false };
}

/** What a kept copy (localStorage) holds back to a state, anything not one of them as none known. */
export function keptState(v: unknown): CapabilityState {
  if (!v || typeof v !== 'object') return NO_CAPABILITIES;
  const o = v as Record<string, unknown>;
  if (o.older === true) return { names: null, older: true };
  return Array.isArray(o.names) && o.names.every((n) => typeof n === 'string') ? { names: o.names as string[], older: false } : NO_CAPABILITIES;
}
