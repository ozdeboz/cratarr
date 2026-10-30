// The guest page's calls (0.10): open, on the home network only (anywhere else the server answers 404, and the page
// says it is not open), a guest known by the name they gave (a cookie of the server's, no account).
import type { QueueTrack } from '../lib/generated/QueueTrack';

/** A track as the guest sees it: as players hold it, with a cover the guest may load. */
export type GuestTrack = QueueTrack & { cover?: string | null };
export type GuestNow = { track: GuestTrack | null; playing?: boolean };
export type GuestAsked = { id: string; track: GuestTrack; how: 'next' | 'last'; at: number; state?: 'pending' | 'accepted' | 'declined' };

export class GuestError extends Error {
  constructor(
    message: string,
    readonly status: number,
  ) {
    super(message);
  }
}

async function call<T>(path: string, init?: RequestInit): Promise<T> {
  const r = await fetch(path, { cache: 'no-store', ...init });
  if (!r.ok) {
    let why = '';
    try {
      why = (await r.json()).error ?? '';
    } catch {
      /* no words */
    }
    throw new GuestError(why || (r.status === 429 ? 'Not so fast: try again in a moment.' : 'Something went wrong.'), r.status);
  }
  return r.status === 204 ? (undefined as T) : r.json();
}
const post = (body: unknown): RequestInit => ({ method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });

export const hello = (name: string) => call<void>('/api/guest/hello', post({ name }));
export const now = () => call<GuestNow>('/api/guest/now');
export const search = (q: string) => call<{ tracks: GuestTrack[] }>(`/api/guest/search?${new URLSearchParams({ q })}`);
export const ask = (track: QueueTrack, how: 'next' | 'last') => call<{ id: string; state: 'pending' | 'accepted' }>('/api/guest/queue-requests', post({ track, how }));
export const mine = () => call<GuestAsked[]>('/api/guest/queue-requests');
