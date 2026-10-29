<script lang="ts">
  // The Library's kind of record, beside back (lib/GlassPicker; lib/recordKinds): Records, Albums, EPs, Singles,
  // Compilations, Other; a Type row made in the Filter sheet that is none of them shows as "Some types".
  import GlassPicker from './GlassPicker.svelte';
  import { chooseKind, kindNow, RECORD_KINDS, type RecordKind } from './recordKinds';

  const chosen = $derived(kindNow());
  const shown = $derived(RECORD_KINDS.find((k) => k.id === chosen)?.label ?? 'Some types');
</script>

<GlassPicker {shown} menuLabel="The Library shows" options={RECORD_KINDS} {chosen} onchoose={(id) => chooseKind(id as RecordKind)} />
