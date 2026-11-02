<script lang="ts">
  // A saved crate's head in the panel (0.13; the owner, 2026-10-31: "one place where to show contents or building of a
  // crate, and that is in the panel"): the builder's sentence (lib/CrateSentence) over its tracks, so a crate looks the
  // same seen as made. Its first change (a term, a join, the curve, words typed) opens its edit mode (lib/CrateEditor)
  // with that change in its draft, where Save and Cancel are; what the sentence cannot say opens there too.
  import { untrack } from 'svelte';
  import { getCrate } from './api';
  import { EDIT_CRATE, keepDraft } from './crates.svelte';
  import CrateSentence from './CrateSentence.svelte';
  import { viewPlaylist } from './panel.svelte';
  import { chipsOf, conditionsOf, type Chip, type Modes, type Named } from './quickCrate';

  let { crate, name }: { crate: string; name: string } = $props();

  let read = $state<{ chips: Chip[]; min: number; modes: Modes; names: Record<string, Named>; rest: import('./api').Condition[]; brief: { text: string; condition: import('./api').Condition } | null } | null>(null);
  let chips = $state<Chip[]>([]);
  let min = $state(0);
  let modes = $state<Modes>({});
  $effect(() => {
    const c = crate;
    let gone = false;
    read = null;
    getCrate(c).then(
      (d) => {
        if (gone) return;
        const names = d.names ?? {};
        const r = chipsOf(d.conditions, names, untrack(() => name));
        [chips, min, modes] = [r.chips, r.min, r.modes];
        read = { chips: r.chips, min: r.min, modes: r.modes, names, rest: r.rest, brief: r.brief };
      },
      () => {},
    );
    return () => (gone = true);
  });

  /** Into its edit mode, the change in its draft. */
  function edit() {
    const r = read;
    if (!r) return;
    const conditions = [...conditionsOf(chips, min, modes), ...(r.brief ? [r.brief.condition] : []), ...r.rest];
    keepDraft(crate, { name, conditions, names: r.names });
    viewPlaylist(EDIT_CRATE + crate);
  }
  // A change in the sentence (words typed at its end too, once read: Enter or a comma): the crate being changed.
  $effect(() => {
    const r = read;
    const now = JSON.stringify([chips, min, modes]);
    if (!r) return;
    untrack(() => {
      if (now !== JSON.stringify([r.chips, r.min, r.modes])) edit();
    });
  });
</script>

<!-- Only where the sentence says something: a crate of conditions alone keeps its plain head. -->
{#if read && (read.chips.length || read.brief)}
  <div class="head">
    <CrateSentence bind:chips bind:min bind:modes rest={read.rest} brief={read.brief?.text ?? null} onmore={() => edit()} />
  </div>
{/if}

<style>
  .head {
    display: grid;
    gap: 10px;
    margin: 4px 0 8px;
  }
</style>
