<script lang="ts">
  // Written to files (decision 0097; the owner, 2026-11-02: a sheet in Catalogue, "keeps it nicely leveled"): per kind,
  // whether Cratarr's words are written into the files' tags, a switch each; genre, types and years on to begin with.
  // Nothing is written without you: Health lists the records whose files differ, and writes them when you tap Write.
  // Types and years are one switch (0.13's, which also writes Classical as a genre).
  import MenuSheet from '../MenuSheet.svelte';
  import { KIND_NAMES, kindOf, setWrite, words } from '../words.svelte';
  import type { CatalogueKind } from '../api';
  import Group from './Group.svelte';
  import Item from './Item.svelte';

  let { onclose }: { onclose: () => void } = $props();
  const ROWS: { kind: CatalogueKind; name: string; field: string }[] = [
    { kind: 'type', name: 'Types and years', field: 'Release type, date, and Classical as a genre' },
    { kind: 'genre', name: KIND_NAMES.genre, field: 'GENRE' },
    { kind: 'style', name: KIND_NAMES.style, field: 'STYLE' },
    { kind: 'mood', name: KIND_NAMES.mood, field: 'MOOD' },
    { kind: 'tag', name: KIND_NAMES.tag, field: 'GROUPING' },
  ];
  const MORE =
    "Cratarr's words written into its files' tags, in every library, so other apps read them as Cratarr does. Nothing is written without you: Health says which records' files differ, and writes them when you tap Write.";
</script>

<MenuSheet label="Written to files" title="Written to files" {onclose}>
  <div class="rows">
    <Group more={MORE}>
      {#each ROWS as r (r.kind)}
        {@const k = kindOf(r.kind)}
        {#if k}
          <Item name={r.name} line={r.field} toggle={{ checked: k.write, onchange: (on) => void setWrite(r.kind, on) }} />
        {/if}
      {/each}
    </Group>
    {#if words.error && !words.value}<Group><Item lead="bad" name={words.error} wrap /></Group>{/if}
  </div>
</MenuSheet>

<style>
  .rows {
    padding: 0 16px 16px;
  }
</style>
