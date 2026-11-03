import type { Collection } from './api';

/** A collection as a player or an app reports it (Radio's and Autoplay's `within`, decision 0094); null for anything
 *  else. */
export function collectionIn(v: unknown): Collection | null {
  if (!v || typeof v !== 'object') return null;
  const o = v as Record<string, unknown>;
  const s = (k: string) => typeof o[k] === 'string' && o[k] !== '';
  switch (o.kind) {
    case 'crate':
      return s('id') ? { kind: 'crate', id: o.id as string, ...(Array.isArray(o.without) ? { without: o.without.filter((x): x is string => typeof x === 'string') } : {}) } : null;
    case 'playlist':
      return s('id') ? { kind: 'playlist', id: o.id as string } : null;
    case 'album':
      return s('source') && s('id') ? { kind: 'album', source: o.source as string, id: o.id as string } : null;
    case 'artist':
      return s('source') && s('name')
        ? { kind: 'artist', source: o.source as string, name: o.name as string, ...(Array.isArray(o.without) ? { without: o.without.filter((x): x is string => typeof x === 'string') } : {}) }
        : null;
    case 'tracks':
      return s('source') && Array.isArray(o.ids) ? { kind: 'tracks', source: o.source as string, ids: o.ids.filter((x): x is string => typeof x === 'string') } : null;
  }
  return null;
}
