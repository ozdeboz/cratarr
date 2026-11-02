<script lang="ts">
  // The Library's release types, beside back (lib/GlassPicker; lib/recordTypes): Records, then every type with records
  // with how many, most first; Type rows that no one type names (made in the Filter sheet) read "Selected types" and
  // tick nothing, and a pick replaces them.
  import GlassPicker from './GlassPicker.svelte';
  import { chooseType, typeNow, typesLabel, typesOffered } from './recordTypes.svelte';
</script>

<GlassPicker shown={typesLabel()} menuLabel="The Library shows" options={typesOffered()} chosen={typeNow()} onchoose={(id) => chooseType(id)} />
