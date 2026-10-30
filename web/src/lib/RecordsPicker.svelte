<script lang="ts">
  // The Library's kind of record, beside back (lib/GlassPicker; lib/recordKinds): Records, Albums, EPs, Singles,
  // Compilations, Classical (0.10, where the server knows it), Other; a Type row made in the Filter sheet that is none of
  // them shows as "Some types".
  import GlassPicker from './GlassPicker.svelte';
  import { chooseKind, kindNow, recordKinds, type RecordKind } from './recordKinds';

  const chosen = $derived(kindNow());
  const kinds = $derived(recordKinds());
  const shown = $derived(kinds.find((k) => k.id === chosen)?.label ?? 'Some types');
</script>

<GlassPicker {shown} menuLabel="The Library shows" options={kinds} {chosen} onchoose={(id) => chooseKind(id as RecordKind)} />
