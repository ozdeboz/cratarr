<script lang="ts" module>
  // The library's words of each kind (the crate builder's values, most common first), read once for every Edit.
  import { getCrateAxes, type WordKind } from './api';
  import { prefs } from './prefs.svelte';
  const knownOf = $state<{ value: Partial<Record<WordKind, string[]>> | null }>({ value: null });
  let asked = false;
  function readKnown() {
    if (asked) return;
    asked = true;
    getCrateAxes(prefs.source || undefined).then(
      (axes) => (knownOf.value = Object.fromEntries((['genre', 'style', 'mood', 'tag'] as const).map((k) => [k, axes.find((x) => x.key === k)?.values ?? []]))),
      () => (asked = false),
    );
  }
</script>

<script lang="ts">
  // A step of an Edit sheet (lib/RecordEdit, lib/TrackEdit; decision 0097, the words' hand layer): one kind's words set
  // by hand. A field to find one, the chosen first as pressed chips, then the library's of that kind; Enter adds what is
  // typed (as the library spells it where it has it). Mood is AudioMuse's words only (the owner, 2026-11-02: one
  // meaning per kind): chosen among them, never typed. `reset`: back to what the words were before the hand.
  import MenuEntry from './MenuEntry.svelte';

  let {
    kind,
    chosen,
    onchange,
    reset = null,
  }: {
    kind: WordKind;
    chosen: string[];
    onchange: (words: string[]) => void;
    reset?: { label: string; onclick: () => void } | null;
  } = $props();

  const ONE: Record<WordKind, string> = { genre: 'genre', style: 'style', mood: 'mood', tag: 'tag' };
  const typable = $derived(kind !== 'mood');
  const what = $derived(typable ? `Find or add a ${ONE[kind]}` : `Find a ${ONE[kind]}`);
  let find = $state('');
  readKnown();

  const chips = $derived.by(() => {
    const q = find.trim().toLowerCase();
    const rest = (knownOf.value?.[kind] ?? []).filter((g) => !chosen.includes(g) && (!q || g.toLowerCase().includes(q)));
    return [...chosen, ...rest.slice(0, q ? 60 : 24)];
  });
  const toggle = (g: string) => onchange(chosen.includes(g) ? chosen.filter((x) => x !== g) : [...chosen, g]);
  function add(e: KeyboardEvent) {
    if (e.key !== 'Enter' || !typable) return;
    e.preventDefault();
    const g = find.trim().replace(/\s+/g, ' ');
    if (!g) return;
    const has = (knownOf.value?.[kind] ?? []).find((k) => k.toLowerCase() === g.toLowerCase()) ?? g;
    if (!chosen.includes(has)) onchange([...chosen, has]);
    find = '';
  }
</script>

<div class="step field">
  <input type="search" placeholder={what} aria-label={what} bind:value={find} onkeydown={add} />
</div>
<div class="step chips">
  {#each chips as g (g)}
    <button class="chip" aria-pressed={chosen.includes(g)} onclick={() => toggle(g)}>{g}</button>
  {/each}
</div>
{#if reset}<MenuEntry icon="reset" label={reset.label} onclick={reset.onclick} />{/if}

<style>
  .step {
    padding: 0 16px 8px;
  }

  .field {
    display: grid;
  }

  /* The term sheet's field (lib/QuickTermSheet). */
  input {
    min-height: 36px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    color: var(--text);
    font: inherit;
    font-size: 15px;
  }

  /* A filter's chips (lib/ConditionCard). */
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    min-height: 32px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg);
    color: inherit;
    font: inherit;
    font-size: 14px;
    cursor: pointer;
  }

  .chip[aria-pressed='true'] {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 22%, var(--bg));
  }
</style>
