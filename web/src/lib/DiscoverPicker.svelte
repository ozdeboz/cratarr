<script lang="ts">
  // Discover's mode, beside back (lib/GlassPicker; lib/discoverModes; the owner, 2026-10-29): Fresh, Alike, Gaps, New
  // artists, and under them the account's Spotify lists once it has them.
  import GlassPicker, { type PickerOption } from './GlassPicker.svelte';
  import { DISCOVER_MODES, modeLabel, SPOTIFY_VIEWS, type DiscoverMode } from './discoverModes';
  import { prefs } from './prefs.svelte';
  import { loadSpotify, spotify } from './spotify.svelte';

  loadSpotify();
  const options = $derived<PickerOption[]>([
    ...DISCOVER_MODES.map((m) => ({ id: m.value, label: m.label })),
    ...(spotify.state?.shown ? SPOTIFY_VIEWS.map((v) => ({ id: v.value, label: v.label, group: 'Spotify' })) : []),
  ]);
</script>

<GlassPicker shown={modeLabel(prefs.discoverMode)} menuLabel="Discover shows" {options} chosen={prefs.discoverMode} onchoose={(id) => (prefs.discoverMode = id as DiscoverMode)} />
