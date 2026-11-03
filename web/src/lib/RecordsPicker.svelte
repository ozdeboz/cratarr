<script lang="ts">
  // The Library's release types, beside back (lib/GlassPicker; lib/recordTypes): Records, then every type with records
  // with how many, most first; Type rows that no one type names (made in the Filter sheet, or several taken in) read
  // "Selected types" and tick each type they select (the owner, 2026-11-02). A tap picks one alone; a long-press takes
  // one in or out (the owner: "a quick way to modify the selection").
  import GlassPicker from './GlassPicker.svelte';
  import { chooseType, toggleType, typesLabel, typesOffered, typesTickedNow } from './recordTypes.svelte';
</script>

<GlassPicker shown={typesLabel()} menuLabel="The Library shows" options={typesOffered()} chosen={typesTickedNow()} onchoose={(id) => chooseType(id)} onhold={toggleType} />
