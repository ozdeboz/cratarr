<script lang="ts">
  // A quick crate's term, tapped in its sentence (lib/QuickCrate): the other ways its word reads, an artist's Sounds
  // like or By and their own tracks, another word for it, Remove. As the 0.11 sheet's chip step was.
  import { chipOf, readAs, readingsFor, type Chip, type Found, type Reading } from './quickCrate';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import Seg from './Seg.svelte';

  let {
    chip,
    ask,
    onchange,
    onremove,
    onclose,
  }: {
    chip: Chip;
    /** Search for a word (its readings come from it). */
    ask: (phrase: string) => Promise<Found>;
    onchange: (chip: Chip) => void;
    onremove: () => void;
    onclose: () => void;
  } = $props();

  const KIND: Record<Reading['kind'], string> = { artist: 'Artist', album: 'Album', track: 'Track', label: 'Label', genre: 'Genre', style: 'Style', mood: 'Mood', tag: 'Tag', year: 'Year' };
  const shown = (r: Reading) => `${KIND[r.kind]} · ${r.label}`;
  /** Its other readings shown: the best few. */
  const OTHERS = 8;

  /** Its word typed again: read as one term, the chip in its place; or nothing found. */
  let again = $state('');
  let missed = $state<string | null>(null);
  async function reread(e: KeyboardEvent) {
    if (e.key !== 'Enter') return;
    e.preventDefault();
    const term = again.trim().replace(/\s+/g, ' ');
    if (!term) return;
    const found = await ask(term).catch(() => [] as Found);
    const c = chipOf(term, readingsFor(term, found));
    if (!c) return void (missed = term);
    onchange(c);
    onclose();
  }
</script>

<MenuSheet label="A word of the quick crate" icon="crates" title={chip.reading.label} {onclose}>
  {#if chip.reading.kind === 'artist' && chip.reading.id}
    <div class="how">
      <Seg
        label="Like or by"
        options={[
          { value: 'like', label: 'Sounds like' },
          { value: 'by', label: 'By' },
        ]}
        bind:value={() => chip.how ?? 'like', (v) => onchange({ ...chip, how: v as 'like' | 'by' })}
      />
    </div>
    {#if chip.how === 'like'}
      <MenuEntry icon="user" label="Their own tracks" switched={!!chip.own} onclick={() => onchange({ ...chip, own: !chip.own })} />
    {/if}
  {/if}
  {#each chip.others.slice(0, OTHERS) as r (r.kind + r.value)}
    <MenuEntry label={shown(r)} onclick={() => (onchange(readAs(chip, r)), onclose())} />
  {/each}
  <div class="again">
    <input type="search" placeholder="Another word for this" aria-label="Another word for this" bind:value={again} onkeydown={reread} />
    {#if missed}<p class="note">Nothing found for {missed}</p>{/if}
  </div>
  <MenuEntry icon="close" label="Remove" danger onclick={() => (onremove(), onclose())} />
</MenuSheet>

<style>
  .how,
  .again {
    display: grid;
    gap: 8px;
    padding: 4px 16px 8px;
  }

  input[type='search'] {
    min-height: 36px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    color: var(--text);
    font: inherit;
    font-size: 15px;
  }

  .note {
    margin: 0;
    color: var(--muted);
    font-size: 13px;
  }
</style>
