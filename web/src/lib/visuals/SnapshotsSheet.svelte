<script lang="ts">
  // The saved looks (decision 0046, amended), from the visuals' bookmark: the app's standard sheet (lib/MenuSheet), a
  // gallery of small pictures. Save first (every slider, and a picture of the visuals now, named after the track and the time, its name
  // then ready to change), Standard second (Cratarr's own, not to be renamed or deleted: shared.svelte.ts), then the
  // saved ones, newest first. A tap brings one back (the visuals' `recall`); one held is renamed, saved over with the
  // visuals as they are now (its name and place kept: the owner, 2026-10-18), or deleted. The
  // owner: "when you press the bookmark button, a sheet opens, because that's our standard format of display". After
  // them, under their heading, the example racks: the one library the panel's Presets read too (library.ts; the owner,
  // 2026-10-29: "This is where snapshots should be"). An example is Cratarr's own, as Standard: a tap brings it, it is
  // not renamed or deleted.
  import { deleteVisualsSnapshot, getVisualsSnapshots, renameVisualsSnapshot, saveOverVisualsSnapshot, saveVisualsSnapshot, type VisualsSnapshot } from '../api';
  import Cover from '../Cover.svelte';
  import Icon from '../Icon.svelte';
  import MenuSheet from '../MenuSheet.svelte';
  import { toast } from '../toast.svelte';
  import { held } from './held.svelte';
  import { libraryGroups, libraryOf } from './library';
  import { longPress } from './longPress';
  import { snapshotNow, STANDARD } from './shared.svelte';

  let {
    onclose,
    capture,
    trackName,
    recall,
    onexample,
    over,
    settingsNow = snapshotNow,
  }: {
    onclose: () => void;
    /** A small picture of the visuals now (a JPEG data URL), or null where none could be taken. */
    capture: () => Promise<string | null>;
    trackName: string | null;
    recall: (snap: VisualsSnapshot) => void;
    /** An example rack chosen (by its id), as the panel's Presets choose one. */
    onexample: (id: string) => void;
    /** What a snapshot keeps (0057's switch: the patch, snapshot-v2.ts); the settings' by default. */
    settingsNow?: () => Record<string, unknown>;
    /** Over the full-screen visuals (not beside the page). */
    over: boolean;
  } = $props();

  let list = $state<VisualsSnapshot[]>([]);
  let saving = $state(false);
  let editing = $state<string | null>(null);
  let editName = $state('');

  getVisualsSnapshots().then(
    (got) => (list = got),
    () => {},
  );
  // The library's other groups (the examples), after the saved ones this sheet keeps itself.
  const others = $derived(libraryGroups(libraryOf(list)).filter((g) => g.id !== 'saved'));

  const time = () => {
    const d = new Date();
    return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  };

  async function save() {
    if (saving) return;
    saving = true;
    try {
      const picture = await capture();
      const made = await saveVisualsSnapshot(`${trackName ?? 'Visuals'} · ${time()}`, settingsNow(), picture);
      list = [made, ...list.filter((x) => x.id !== made.id)];
      // Its name at once, ready to change (Enter or a tap away keeps it).
      edit(made);
    } catch (e) {
      toast.fail(e, 'Not saved');
    } finally {
      saving = false;
    }
  }
  function edit(snap: VisualsSnapshot) {
    editName = snap.name;
    editing = snap.id;
  }
  async function rename(snap: VisualsSnapshot) {
    if (editing !== snap.id) return;
    editing = null;
    const name = editName.trim();
    if (!name || name === snap.name) return;
    try {
      const done = await renameVisualsSnapshot(snap.id, name);
      list = list.map((x) => (x.id === snap.id ? { ...x, ...done } : x));
    } catch (e) {
      toast.fail(e, 'Not renamed');
    }
  }
  /** Saved over with every setting and a picture of the visuals now; a name being changed goes with it. */
  async function saveOver(snap: VisualsSnapshot) {
    if (saving) return;
    const name = editName.trim();
    editing = null;
    saving = true;
    try {
      const picture = await capture();
      const done = await saveOverVisualsSnapshot(snap.id, settingsNow(), picture, name && name !== snap.name ? name : undefined);
      list = list.map((x) => (x.id === snap.id ? { ...x, ...done } : x));
    } catch (e) {
      toast.fail(e, 'Not saved');
    } finally {
      saving = false;
    }
  }
  async function remove(snap: VisualsSnapshot) {
    editing = null;
    try {
      await deleteVisualsSnapshot(snap.id);
      list = list.filter((x) => x.id !== snap.id);
    } catch (e) {
      toast.fail(e, 'Not deleted');
    }
  }
  const focusNow = (node: HTMLInputElement) => {
    node.focus();
    node.select();
  };
</script>

<MenuSheet label="Snapshots" icon="bookmark" title="Snapshots" {over} {onclose}>
  <div class="tiles">
    <button class="tile" disabled={saving} use:held={'snapshots:save'} onclick={save}>
      <span class="pic glyph"><Icon name="plus" size={28} /></span>
      <span class="name">Save</span>
    </button>
    <button class="tile" use:held={'snapshots:standard'} onclick={() => recall(STANDARD as unknown as VisualsSnapshot)}>
      <span class="pic glyph standard"><Icon name="visuals" size={28} /></span>
      <span class="name">{STANDARD.name}</span>
    </button>
    {#each list as snap (snap.id)}
      {#if editing === snap.id}
        <div class="tile editing">
          <span class="pic"><Cover src={snap.picture} /></span>
          <span class="edit">
            <input
              aria-label="Name"
              maxlength="200"
              bind:value={editName}
              use:focusNow
              onkeydown={(e) => {
                if (e.key === 'Enter') rename(snap);
                else if (e.key === 'Escape') {
                  e.stopPropagation();
                  editing = null;
                }
              }}
              onblur={() => rename(snap)}
            />
            <button class="over" aria-label="Save over {snap.name}" use:held={'snapshots:over'} disabled={saving} onpointerdown={(e) => e.preventDefault()} onclick={() => saveOver(snap)}
              ><Icon name="save" size={16} /></button
            >
            <button class="delete" aria-label="Delete {snap.name}" onpointerdown={(e) => e.preventDefault()} onclick={() => remove(snap)}><Icon name="trash" size={16} /></button>
          </span>
        </div>
      {:else}
        <button class="tile" use:longPress={() => (snap.shared ? undefined : edit(snap))} onclick={() => recall(snap)}>
          <span class="pic"><Cover src={snap.picture} /></span>
          <span class="name">{snap.name}</span>
        </button>
      {/if}
    {/each}
    {#each others as g (g.id)}
      <h3>{g.label}</h3>
      {#each g.entries as x (x.id)}
        <button class="tile" title={x.line} onclick={() => onexample(x.id)}>
          <span class="pic glyph standard"><Icon name="visuals" size={28} /></span>
          <span class="name">{x.label}</span>
        </button>
      {/each}
    {/each}
  </div>
</MenuSheet>

<style>
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(132px, 1fr));
    gap: 12px;
    padding: 4px 16px 8px;
    overflow-y: auto;
  }

  .tile {
    display: grid;
    gap: 6px;
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  /* A group's heading (Examples) across the whole row, as the panel's. */
  h3 {
    grid-column: 1 / -1;
    margin: 8px 0 0;
    font-size: 0.85rem;
    font-weight: 600;
  }

  .tile:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .pic {
    display: grid;
    place-items: center;
    aspect-ratio: 16 / 9;
    overflow: hidden;
    border-radius: 8px;
    background: rgb(255 255 255 / 0.06);
    color: rgb(255 255 255 / 0.7);
    /* A snapshot without a picture: lib/Cover's stand-in, in the sheet's own white. */
    --cover-tile: transparent;
    --cover-mark: rgb(255 255 255 / 0.28);
  }

  .glyph {
    border: 1px dashed rgb(255 255 255 / 0.25);
    background: none;
  }

  .glyph.standard {
    border-style: solid;
    background: radial-gradient(circle at 50% 55%, rgb(120 90 220 / 0.45), transparent 70%), #000;
  }

  .name {
    overflow: hidden;
    font-size: 0.85rem;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .edit {
    display: flex;
    gap: 4px;
    align-items: center;
  }

  .edit input {
    flex: 1;
    min-width: 0;
    min-height: 30px;
    padding: 0 8px;
    border: 1px solid rgb(255 255 255 / 0.5);
    border-radius: 8px;
    background: rgb(255 255 255 / 0.08);
    color: #fff;
    font: inherit;
    font-size: 0.85rem;
  }

  .edit .over,
  .edit .delete {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border: 0;
    border-radius: 8px;
    background: rgb(232 112 143 / 0.25);
    color: #fff;
    cursor: pointer;
  }

  .edit .over {
    background: rgb(255 255 255 / 0.12);
  }
</style>
