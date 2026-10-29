// A patch made by toV2, kept from before (on this device, in a snapshot) or sent by the phone, made current as it loads:
// each change of the model since, applied once more, every one of them idempotent (decision 0077: macros one level
// down; the shipped devices named by what they carry).
import { namedDevices } from './device-names';
import { macrosOneLevel } from './macro-levels';
import type { ModuleType } from './patch';
import type { PatchV2 } from './ports';

/** `p` made current (in place), and `p`. */
export const upgraded = (p: PatchV2, types: Record<string, ModuleType>): PatchV2 => namedDevices(macrosOneLevel(p), types);
