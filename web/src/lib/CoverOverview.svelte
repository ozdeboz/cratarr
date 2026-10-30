<script lang="ts">
  // The cover's overview (the owner, 2026-10-19): a circle drawn on the full player's cover, or the info button at the
  // start of its row, blurs the cover and shows over it every fact known about the track (lib/trackFacts), in full and
  // in the owner's order: the genres as chips on top, energy and danceability as bars. Only what is known shows. Drawn
  // by lib/CoverFacts, as a flow's details are.
  import type { TrackInfo } from './api';
  import CoverFacts from './CoverFacts.svelte';
  import { detailsOf, factsOf, FACTS } from './trackFacts.svelte';

  let { track }: { track: { source: string; id: string; info?: TrackInfo | null } } = $props();

  const facts = $derived(factsOf(detailsOf(track.source, track.id), track.info ?? null));
  const genres = $derived(facts.genre?.full.split(' · ') ?? []);
  const lines = $derived(FACTS.filter((f) => f.id !== 'genre' && facts[f.id]).map((f) => ({ id: f.id, label: f.label, full: facts[f.id]!.full, meter: facts[f.id]!.meter })));
</script>

<CoverFacts {genres} {lines} />
