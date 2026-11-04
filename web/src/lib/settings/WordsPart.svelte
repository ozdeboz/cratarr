<script lang="ts">
  // Sources and spellings (decision 0097, the controls; the owner, 2026-11-03: a row in Catalogue, a page per kind): for
  // genre, style, mood and tags, each source on or off (off: its words leave the kind everywhere, kept to come back), and
  // the spellings that are one word (case, spaces and hyphens fold by themselves). `thing`: 'words', the kinds;
  // 'words:<kind>', one kind's page (LibrariesPart names them). What is written into the files is Catalogue's "Written
  // to files" sheet's (lib/settings/WrittenSheet).
  import type { WordKind } from '../api';
  import { openThing } from '../panel.svelte';
  import { KIND_NAMES, kindOf, readWords, setSource, setSpellings, SPELLED, words } from '../words.svelte';
  import Notice from '../Notice.svelte';
  import PageLoading from '../PageLoading.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import ValueSheet from './ValueSheet.svelte';

  let { thing }: { thing: string } = $props();
  const kind = $derived(SPELLED.find((w) => thing === `words:${w}`) ?? null);
  const k = $derived(kind ? kindOf(kind) : null);

  const MORE = "Off, a source's words leave this kind everywhere, and come back when it is on again. What you set by hand always counts.";
  const SPELL_MORE = 'Other spellings of the same word, each read as the word it belongs to, everywhere. Case, spaces and hyphens fold by themselves.';
  const lineOf = (w: WordKind) => {
    const x = kindOf(w);
    if (!x) return null;
    const on = x.sources.filter((s) => s.on).length;
    return [`${on} of ${x.sources.length} sources`, x.aliases.length ? `${x.aliases.length} with other spellings` : null].filter(Boolean).join(' · ');
  };

  /** The word whose spellings are typed; '' while a new word is named first. */
  let spelling = $state<string | null>(null);
  let naming = $state(false);
</script>

{#if words.error && !words.value}
  <Notice text={words.error} onretry={readWords} />
{:else if !words.value}
  <PageLoading />
{:else if !kind}
  <Group>
    {#each SPELLED as w (w)}
      <Item name={KIND_NAMES[w]} line={lineOf(w)} onclick={() => openThing('libraries', `words:${w}`)} />
    {/each}
  </Group>
{:else if k}
  {@const w = kind}
  <Group title="Sources" more={MORE}>
    {#each k.sources as s (s.id)}
      <Item name={s.name} disabled={s.id === 'hand'} toggle={{ checked: s.on, onchange: (on) => void setSource(w, s.id, on) }} />
    {/each}
  </Group>
  <Group title="Spelled as one" more={SPELL_MORE}>
    {#each k.aliases as a (a.word)}
      <Item name={a.word} line={a.aliases.join(', ')} onclick={() => (spelling = a.word)} />
    {/each}
    <Item name="Add a word" kind="add" onclick={() => (naming = true)} />
  </Group>

  {#if naming}
    <ValueSheet title="Word" value="" help="The spelling that stays." allowEmpty={false} onsave={(v) => void ((naming = false), (spelling = v.trim()))} onclose={() => (naming = false)} />
  {:else if spelling}
    {@const word = spelling}
    <ValueSheet
      title={`Also ${word}`}
      value={k.aliases.find((a) => a.word === word)?.aliases.join('\n') ?? ''}
      help="Other spellings, one a line. None: it has none."
      lines
      onsave={(v) => setSpellings(w, word, v)}
      onclose={() => (spelling = null)}
    />
  {/if}
{/if}
