// The disk floor in Settings › Libraries (AL03, decision 0084: below it, free where users' music lands, users' new Wants wait;
// administrators' go on). Read as the part shows; set at once, the answer the floor read again. A refusal leaves the floor as
// it was read, its words in `error`, and is thrown on, so the sheet it was typed in stays. (GET/PUT /api/settings/floor,
// administrators'.)
import { getFloor, setFloor } from './api';
import type { Floor } from './room';

export const floor = $state({ value: null as Floor | null, error: null as string | null });

const words = (e: unknown) => (e instanceof Error ? e.message : String(e));

export const loadFloor = () =>
  getFloor().then(
    (v) => ((floor.value = v), (floor.error = null)),
    (e) => (floor.error = words(e)),
  );

export async function saveFloor(gb: number) {
  try {
    floor.value = await setFloor(gb);
    floor.error = null;
  } catch (e) {
    floor.error = words(e);
    throw e;
  }
}
