<script lang="ts">
  // A track's Edit (decision 0097, the words' hand layer; administrators'), from its menu: its own words where they
  // differ from its record's, a kind each (Genres, Styles, Moods, Tags), as the record's Edit sets the record's
  // (lib/RecordEdit, its steps lib/WordsStep). A kind it has none of its own of shows its record's, "As its record";
  // "As its record" in a step takes its own back. Save sends only the kinds changed, once.
  import { albumView, setTrackWords, type TrackWords, type WordKind } from './api';
  import Asking from './Asking.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import Group from './settings/Group.svelte';
  import Item from './settings/Item.svelte';
  import { toast } from './toast.svelte';
  import WordsStep from './WordsStep.svelte';

  let { source, id, album, title, artist, onclose }: { source: string; id: string; album: string; title: string; artist: string | null; onclose: () => void } = $props();

  const KINDS: { kind: WordKind; name: string }[] = [
    { kind: 'genre', name: 'Genres' },
    { kind: 'style', name: 'Styles' },
    { kind: 'mood', name: 'Moods' },
    { kind: 'tag', name: 'Tags' },
  ];
  const RECORD = 'As its record';

  // Its record's words and its own (the album view's `words`: the tracks whose words differ, each kind it differs in).
  let read = $state<{ record: Partial<Record<WordKind, string[]>>; own: Partial<Record<WordKind, string[]>> } | null>(null);
  $effect(() => {
    let current = true;
    albumView(source, album).then(
      (v) => {
        if (!current) return;
        const of = (kinds: { kind: string; words: { word: string }[] }[] | undefined) =>
          Object.fromEntries((kinds ?? []).map((k) => [k.kind, k.words.map((w) => w.word)])) as Partial<Record<WordKind, string[]>>;
        read = { record: of(v.words?.kinds), own: of(v.words?.tracks.find((t) => t.id === id)?.kinds) };
      },
      (e) => current && (toast.fail(e, "Its words couldn't be read"), onclose()),
    );
    return () => (current = false);
  });

  let draft = $state<TrackWords>({});
  const changed = $derived(Object.keys(draft).length > 0);
  /** A kind as the sheet shows it: its own (the draft's, else read), or its record's. */
  const shownOf = (k: WordKind) => {
    const d = draft[k];
    if (d !== undefined) return d === null ? { words: read?.record[k] ?? [], own: false } : { words: d, own: true };
    const own = read?.own[k];
    return own ? { words: own, own: true } : { words: read?.record[k] ?? [], own: false };
  };
  const listed = (v: string[]) => (v.length ? (v.length > 2 ? `${v.slice(0, 2).join(', ')} +${v.length - 2}` : v.join(', ')) : '–');

  let step = $state<'rows' | WordKind>('rows');
  let saving = $state(false);
  async function save() {
    if (!changed || saving) return;
    saving = true;
    try {
      await setTrackWords(source, id, draft);
      onclose();
    } catch (e) {
      toast.fail(e, "It wasn't saved");
    } finally {
      saving = false;
    }
  }
</script>

<MenuSheet
  label="Edit {title}"
  {title}
  icon="pencil"
  chips={artist ? [artist] : []}
  step={step === 'rows' ? 'menu' : step}
  end={step === 'rows' ? { icon: 'check', label: 'Save', disabled: !changed, busy: saving, onclick: save } : null}
  onback={step === 'rows' ? null : () => (step = 'rows')}
  {onclose}
>
  {#if step === 'rows'}
    <div class="rows">
      {#if read}
        <Group>
          {#each KINDS as k (k.kind)}
            {@const s = shownOf(k.kind)}
            <Item name={k.name} line={s.own ? 'Its own' : RECORD} value={listed(s.words)} onclick={() => (step = k.kind)} />
          {/each}
        </Group>
      {:else}
        <Asking />
      {/if}
    </div>
  {:else}
    {@const k = step}
    {@const s = shownOf(k)}
    <WordsStep kind={k} chosen={s.words} onchange={(v) => (draft[k] = v)} reset={s.own ? { label: RECORD, onclick: () => ((draft[k] = null), (step = 'rows')) } : null} />
  {/if}
</MenuSheet>

<style>
  .rows {
    padding: 0 12px 12px;
  }
</style>
