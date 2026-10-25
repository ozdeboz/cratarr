<script lang="ts">
  // Radio and Autoplay (the owner, 2026-10-25: a part of its own under Personal, Taste's More music moved here): what
  // plays besides what was chosen, in its sheet (lib/MoreMusicSheet); then what they choose from, apart from what the
  // pages show: each of Cratarr's own libraries, and Bandcamp (Discover's suggestions that play there, heard as
  // previews, decision 0071). Kept for the account (lib/mySettings' more_music, a5: the server reads it for every
  // player of the account).
  import { library, readLibrary } from '../albums.svelte';
  import MoreMusicSheet, { moreMusicParts } from '../MoreMusicSheet.svelte';
  import { loadMine, mine, setMoreFrom } from '../mySettings.svelte';
  import { OWN } from '../sources.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';

  void loadMine();
  if (!library.view) readLibrary(false).catch(() => {});

  let moreMusic = $state(false);
  const libraries = $derived(library.view?.sources.find((s) => s.id === OWN)?.library_info ?? []);
  const isOn = (id: string) => mine.moreFrom.libraries === null || mine.moreFrom.libraries.includes(id);
  /** A library on or off: every one on is all of them (null), as a library added later is then on too. */
  function turn(id: string, on: boolean) {
    const now = libraries.map((l) => l.id).filter((x) => (x === id ? on : isOn(x)));
    setMoreFrom({ libraries: now.length === libraries.length ? null : now });
  }
</script>

<Group title="Radio and Autoplay" more="What plays besides what you chose, on this device's own player: Radio's tracks after each track, and Autoplay when the queue ends.">
  <Item name="More music" value={moreMusicParts().map((p) => p.text).join(' · ') || 'Off'} onclick={() => (moreMusic = true)} />
</Group>
<Group title="From" more="Where Radio and Autoplay find their music, whatever the pages show. Bandcamp: Discover's suggestions that play there, heard as previews; never counted as plays.">
  {#each libraries as l (l.id)}
    <Item name={l.name} toggle={{ checked: isOn(l.id), onchange: (on) => turn(l.id, on) }} />
  {/each}
  <Item name="Bandcamp" toggle={{ checked: mine.moreFrom.bandcamp, onchange: (on) => setMoreFrom({ bandcamp: on }) }} />
</Group>
{#if moreMusic}
  <MoreMusicSheet onclose={() => (moreMusic = false)} />
{/if}
