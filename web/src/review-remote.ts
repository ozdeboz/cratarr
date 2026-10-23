// Review harness: a track playing, for its cover.
export * from './lib/remote.svelte';
export const playing = () => ({ source: 'review', id: 'cover' }) as any;
