<script lang="ts">
  // A record's Edit (0.13; the owner, 2026-10-31: per-record overrides, administrators'), from its menu (lib/AlbumMenu)
  // or several ticked ones' (lib/SeveralAlbums): what Cratarr found, set by hand household-wide. Settings' rows
  // (lib/settings/Group and Item) in a menu sheet: Type (a sheet of six: as Cratarr finds it and the five types), Year,
  // Secondary types and Genres each a step of their own, Classical a slim track of three (as Cratarr finds it, yes, no;
  // the 0.10 Classical… sheet folded in); then (decision 0097, the words' hand layer) Styles, Moods and Tags, steps as
  // Genres' is. A row set by hand says so; several that differ say Mixed and are left alone
  // until changed. Save sends only what changed (lib/recordEdit toSend), once; the Library reads itself again after.
  import { albumView, setAlbumOverrides, type ListedAlbum } from './api';
  import { readLibrary } from './albums.svelte';
  import ChoiceList from './ChoiceList.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { plural } from './insights.svelte';
  import {
    RECORD_TYPES,
    SECONDARY_TYPES,
    TYPE_LABEL,
    classicalChoiceOf,
    classicalOfChoice,
    editableOf,
    secondaryLabel,
    WORD_FIELD_NAME,
    WORD_FIELDS,
    WORD_KIND,
    wordsOfView,
    isSecondary,
    shownOf,
    toSend,
    yearOf,
    type ClassicalChoice,
    type OverrideSet,
    type RecordType,
    type WordField,
  } from './recordEdit';
  import Seg from './Seg.svelte';
  import WordsStep from './WordsStep.svelte';
  import Group from './settings/Group.svelte';
  import Item from './settings/Item.svelte';
  import { toast } from './toast.svelte';

  let { source, albums, onclose }: { source: string; albums: ListedAlbum[]; onclose: () => void } = $props();

  // One record's words read as the sheet opens (the list does not carry them); several's are not asked for: their rows
  // leave each its own until changed.
  let wordsRead = $state<Record<WordField, string[]> | null>(null);
  $effect(() => {
    if (albums.length !== 1) return;
    const id = albums[0].id;
    let current = true;
    albumView(source, id).then(
      (r) => current && (wordsRead = wordsOfView(r)),
      () => {},
    );
    return () => (current = false);
  });
  const records = $derived(albums.map((a) => editableOf(a, albums.length === 1 ? wordsRead : null)));

  let draft = $state<OverrideSet>({});
  const send = $derived(toSend(records, draft));
  const changed = $derived(Object.keys(send).length > 0);

  let step = $state<'rows' | 'type' | 'year' | 'secondary' | WordField>('rows');

  const SET = 'Set by hand';
  const FOUND = 'As Cratarr finds it';
  const lineOf = (s: { mixed: boolean; set: boolean; reset: boolean }) => (s.reset ? FOUND : s.mixed ? 'Mixed' : s.set ? SET : null);

  // Type.
  const type = $derived(shownOf(records, draft, 'type'));
  const typeLabel = (v: string | null) => (v && v in TYPE_LABEL ? TYPE_LABEL[v as RecordType] : '–');
  const TYPE_CHOICES: { value: RecordType | 'auto'; label: string }[] = [{ value: 'auto', label: FOUND }, ...RECORD_TYPES.map((t) => ({ value: t, label: TYPE_LABEL[t] }))];
  const typeChoice = $derived<RecordType | 'auto' | ''>(type.reset ? 'auto' : type.mixed ? '' : type.set ? ((type.value as RecordType | null) ?? 'auto') : 'auto');

  // Year.
  const year = $derived(shownOf(records, draft, 'year'));
  let yearTyped = $state('');
  function openYear() {
    yearTyped = year.value === null || year.mixed ? '' : String(year.value);
    step = 'year';
  }
  const yearOk = $derived(yearOf(yearTyped) !== null);

  // Secondary types: MusicBrainz's twelve only, the ones Cratarr keeps (a5, 2026-10-31: any other is refused); one a
  // record carries from elsewhere is not offered and not sent back.
  const secondary = $derived(shownOf(records, draft, 'secondary'));
  const secondaryNow = $derived<string[]>(((secondary.mixed || secondary.reset ? [] : (secondary.value as string[] | null)) ?? []).filter(isSecondary));
  const secondaryOffered = SECONDARY_TYPES;
  const toggleSecondary = (v: string) => (draft.secondary = secondaryNow.includes(v) ? secondaryNow.filter((x) => x !== v) : [...secondaryNow, v]);

  // The words, a kind each (genres, styles, moods, tags), each a step of its own (lib/WordsStep).
  const isWord = (x: string): x is WordField => (WORD_FIELDS as readonly string[]).includes(x);
  const shownWords = $derived(Object.fromEntries(WORD_FIELDS.map((f) => [f, shownOf(records, draft, f)])) as Record<WordField, ReturnType<typeof shownOf<WordField>>>);
  const nowOf = (f: WordField): string[] => {
    const w = shownWords[f];
    return (w.mixed || w.reset ? [] : (w.value as string[] | null)) ?? [];
  };
  const listed = (v: string[], label: (x: string) => string = (x) => x) => (v.length ? (v.length > 2 ? `${v.slice(0, 2).map(label).join(', ')} +${v.length - 2}` : v.map(label).join(', ')) : '–');

  // Classical: three choices, a slim track.
  const CLASSICAL_CHOICES: { value: ClassicalChoice; label: string }[] = [
    { value: 'auto', label: 'Auto' },
    { value: 'yes', label: 'Yes' },
    { value: 'no', label: 'No' },
  ];
  const classical = $derived(classicalChoiceOf(records, draft));
  const classicalFound = $derived(classical === 'auto' && !('classical' in draft) && records.length === 1 && records[0].classical !== null ? (records[0].classical ? 'Found classical' : 'Found not classical') : null);

  /** A field back to what Cratarr finds (null), or, where none had it set, the draft's change taken back. */
  function reset(field: keyof OverrideSet) {
    draft[field] = null;
    step = 'rows';
  }
  const anySet = (field: keyof OverrideSet) => records.some((r) => r.set.includes(field)) || (field in draft && draft[field] !== null);

  let saving = $state(false);
  async function save() {
    if (!changed || saving) return;
    saving = true;
    try {
      await setAlbumOverrides(
        albums.map((a) => a.id),
        send,
      );
      onclose();
      readLibrary(true).catch(() => {});
    } catch (e) {
      toast.fail(e, "It wasn't saved");
    } finally {
      saving = false;
    }
  }

  const title = $derived(albums.length === 1 ? albums[0].title : plural(albums.length, 'album'));
</script>

<MenuSheet
  label="Edit {title}"
  {title}
  icon="pencil"
  chips={albums.length === 1 ? [albums[0].artist ?? 'Unknown artist'] : []}
  {step}
  end={step === 'rows' ? { icon: 'check', label: 'Save', disabled: !changed, busy: saving, onclick: save } : null}
  onback={step === 'rows' ? null : () => (step = 'rows')}
  {onclose}
>
  {#if step === 'rows'}
    <div class="rows">
      <Group>
        <Item name="Type" line={lineOf(type)} value={type.mixed || type.reset ? null : typeLabel(type.value as string | null)} onclick={() => (step = 'type')} />
        <Item name="Year" line={lineOf(year)} value={year.mixed || year.reset || year.value === null ? null : String(year.value)} onclick={openYear} />
        <Item name="Secondary types" line={lineOf(secondary)} value={secondary.mixed || secondary.reset ? null : listed(secondaryNow, secondaryLabel)} onclick={() => (step = 'secondary')} />
        {@render wordRow('genres')}
        <Item name="Classical" line={classical === '' ? 'Mixed' : classicalFound}>
          <Seg label="Classical" options={CLASSICAL_CHOICES} bind:value={() => classical, (v) => (draft.classical = classicalOfChoice(v))} />
        </Item>
        {@render wordRow('styles')}
        {@render wordRow('moods')}
        {@render wordRow('tags')}
      </Group>
    </div>
  {:else if step === 'type'}
    <div class="step">
      <ChoiceList
        label="Type"
        options={TYPE_CHOICES}
        value={typeChoice}
        onchange={(v) => {
          draft.type = v === 'auto' || v === '' ? null : v;
          step = 'rows';
        }}
      />
    </div>
  {:else if step === 'year'}
    <div class="step field">
      <input type="number" inputmode="numeric" placeholder="Year" aria-label="Year" bind:value={yearTyped} onkeydown={(e) => e.key === 'Enter' && yearOk && ((draft.year = yearOf(yearTyped)), (step = 'rows'))} />
    </div>
    <MenuEntry icon="check" label="Done" disabled={!yearOk} onclick={() => ((draft.year = yearOf(yearTyped)), (step = 'rows'))} />
    {#if anySet('year')}<MenuEntry icon="reset" label={FOUND} onclick={() => reset('year')} />{/if}
  {:else if step === 'secondary'}
    <div class="step chips">
      {#each secondaryOffered as v (v)}
        <button class="chip" aria-pressed={secondaryNow.includes(v)} onclick={() => toggleSecondary(v)}>{secondaryLabel(v)}</button>
      {/each}
    </div>
    {#if anySet('secondary')}<MenuEntry icon="reset" label={FOUND} onclick={() => reset('secondary')} />{/if}
  {:else if isWord(step)}
    {@const f = step}
    <WordsStep kind={WORD_KIND[f]} chosen={nowOf(f)} onchange={(v) => (draft[f] = v)} reset={anySet(f) ? { label: FOUND, onclick: () => reset(f) } : null} />
  {/if}
</MenuSheet>

{#snippet wordRow(f: WordField)}
  {@const w = shownWords[f]}
  {@const each = albums.length > 1 && !(f in draft)}
  <Item name={WORD_FIELD_NAME[f]} line={w.reset ? FOUND : each ? 'Each its own' : lineOf(w)} value={w.reset || each ? null : listed(nowOf(f))} onclick={() => (step = f)} />
{/snippet}

<style>
  .rows {
    padding: 0 12px 12px;
  }

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
