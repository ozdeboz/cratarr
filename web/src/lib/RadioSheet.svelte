<script lang="ts">
  // One radio sheet, wherever radio starts (decision 0094 #5; the owner, 2026-11-02, its words: "Only this crate / Like
  // it"): where (only this list's tracks, endlessly; or music like it, from the libraries on for Radio), how it moves on
  // (Autoplay's modes, Your taste among them), whole records; Start radio. It opens on the last choices (this device's).
  // Built from the More music sheet's pieces (lib/Card, ChoiceList, Switch).
  import Card from './Card.svelte';
  import ChoiceList from './ChoiceList.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import Switch from './Switch.svelte';
  import { AUTOPLAY_NAMES } from './MoreMusicSheet.svelte';
  import { startRadio, type AutoplayMode } from './localPlayer.svelte';
  import { prefs } from './prefs.svelte';
  import { radioSheet, type RadioAsk } from './radioAsk.svelte';
  import { toast } from './toast.svelte';

  let { ask }: { ask: RadioAsk } = $props();

  const KIND: Record<RadioAsk['kind'], string> = { crate: 'crate', record: 'record', playlist: 'playlist', artist: 'artist', selection: 'selection', track: 'track' };
  const close = () => (radioSheet.ask = null);
  // Opened on the last choices; "Only this …" only where there is a list to stay inside.
  // svelte-ignore state_referenced_locally
  let where = $state<'within' | 'like'>(prefs.radioLast.where === 'within' && ask.within ? 'within' : 'like');
  // svelte-ignore state_referenced_locally
  let mode = $state<AutoplayMode>(prefs.radioLast.mode);
  // svelte-ignore state_referenced_locally
  let albums = $state(prefs.radioLast.albums);
  const WHERE = $derived([
    ...(ask.within ? [{ value: 'within' as const, label: `Only this ${KIND[ask.kind]}` }] : []),
    { value: 'like' as const, label: 'Like it' },
  ]);
  const MODES = (Object.entries(AUTOPLAY_NAMES) as [AutoplayMode, string][]).map(([value, label]) => ({ value, label }));

  let starting = $state(false);
  async function start() {
    if (starting) return;
    prefs.radioLast = { where, mode, albums };
    starting = true;
    try {
      const within = where === 'within' ? ask.within : null;
      await startRadio(await ask.seed(), ask.name, within, { mode, albums });
      close();
    } catch (e) {
      toast.fail(e, "The radio didn't start");
    } finally {
      starting = false;
    }
  }
</script>

<MenuSheet label="Radio" icon="radio" title={ask.name ?? 'Radio'} actions={[{ icon: 'radio', label: 'Start radio', onclick: () => void start(), busy: starting }]} onclose={close}>
  <div class="parts">
    {#if WHERE.length > 1}
      <Card title="Where" label="Where">
        <ChoiceList label="Where" options={WHERE} value={where} onchange={(v) => (where = v)} />
      </Card>
    {/if}
    <Card title="How it moves on" label="How it moves on">
      <ChoiceList label="How it moves on" options={MODES} value={mode} onchange={(v) => (mode = v)} />
    </Card>
    <Switch card label="Whole records" hint="Whole records instead of single tracks." hintWhenOn checked={albums} onchange={(on) => (albums = on)} />
  </div>
</MenuSheet>

<style>
  /* As the More music sheet's: its parts cards (lib/Card), each a choice of one (lib/ChoiceList). */
  .parts {
    --card: color-mix(in srgb, var(--text) 6%, transparent);
    display: grid;
    gap: 8px;
    padding: 4px 16px 16px;
  }
</style>
