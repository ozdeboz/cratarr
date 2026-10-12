// The sources the server has connected, for the source picker. Filled by the status call (any page) and by the
// playlist poll (which also says which are playing).
import type { SourceTab } from './api';

export const known = $state<{ list: SourceTab[] }>({ list: [] });

export function remember(list: SourceTab[]) {
  known.list = list;
}
