// Work in progress in the panel that outlives its view (decisions 0018, 0025): going over the page or back beside it
// builds the playlist view again, so an edit under way and a crate's "Make playlist" name are kept here, for the
// panel's subject they belong to, and the new view carries on with them. Viewing something else (or closing) drops
// them, as before.
import { panel } from './panel.svelte';

export const work = $state<{ editing: { for: string; state: unknown } | null; making: { for: string; state: unknown } | null }>({
  editing: null,
  making: null,
});

$effect.root(() => {
  $effect(() => {
    const viewed = panel.viewed;
    if (work.editing && work.editing.for !== viewed) work.editing = null;
    if (work.making && work.making.for !== viewed) work.making = null;
  });
});
