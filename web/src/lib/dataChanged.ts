// The page's watching of what changed on the server (lib/dataChanges, on the live channel's events).
import { changesOn } from './dataChanges';
import { on } from './events.svelte';

export type { DataKind } from './dataChanges';
export const whenChanged = changesOn(on);
