<script lang="ts">
  // Libraries: each server library (Cratarr's own: each folder, decision 0044) a row with its use and where it is;
  // its own page behind it (`thing`: `lib:<server>:<library>`) with its use and, for a used one, its format and dot
  // (decision 0034); Music's names, how long deleted albums are kept, the catalogue and tag splitting (its own page,
  // `tags`). Saved with the Save bar (the settings' North Star, 2026-10-19).
  import { loadRuntime, runtime, saveRuntime } from '../serverRuntime.svelte';
  import { folderStatus, getCatalogue, type FolderStatus, type ServerLibrary } from '../api';
  import MenuSheet from '../MenuSheet.svelte';
  import { backFromThing, openThing } from '../panel.svelte';
  import Seg from '../Seg.svelte';
  import { nameThing } from '../settingsSections.svelte';
  import ChoiceSheet from './ChoiceSheet.svelte';
  import ConfirmSheet from './ConfirmSheet.svelte';
  import { count, ed, FORMATS, USES, type Draft } from './editor.svelte';
  import FolderBrowser from './FolderBrowser.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import ValueSheet from './ValueSheet.svelte';

  let { thing = null }: { thing?: string | null } = $props();

  // Seg's choices: a use, a format.
  const USE_OPTIONS = USES.map((x) => ({ value: x.id, label: x.label }));
  const FORMAT_OPTIONS = FORMATS.map((f) => ({ value: f.id, label: f.label }));
  const useLabel = (id: string) => USES.find((u) => u.id === id)?.label ?? id;
  const USE_MORE = 'Music: the playlists and the tracks that can go in them. Incoming: new music waiting to be heard.';
  const TAGS_MORE =
    'How tags with several artists or genres in one value are read. The files are never changed: this is only how Cratarr reads them.\n\nArtist separators: "&", "," and "feat." are never split on, as they are part of credits.';

  // A server's name; its id only beside another of its kind.
  const serverLabel = (d: Draft) => `${ed.kindOf(d)?.label ?? d.kind}${ed.drafts.filter((x) => x.kind === d.kind).length > 1 ? ` (${d.id})` : ''}`;

  // Cratarr's own folders (decision 0044): the chosen ones, kept in its settings (`folders`); from before there was a
  // choice, the libraries it found.
  const chosenOf = (d: Draft): string[] =>
    Array.isArray(d.values.folders) ? (d.values.folders as string[]) : (d.view?.server_libraries.map((l) => l.id) ?? []);
  const folderName = (p: string) => p.split('/').filter(Boolean).pop() ?? p;
  let browsing = $state<Draft | null>(null);

  function addFolder(d: Draft, path: string) {
    const now = chosenOf(d);
    if (!now.includes(path)) d.values.folders = [...now, path];
  }

  function removeFolder(d: Draft, path: string) {
    ed.setUse(d, path, 'unused');
    d.values.folders = chosenOf(d).filter((p) => p !== path);
  }

  // Each chosen folder's state, asked for as the list shows.
  let statuses = $state<Record<string, FolderStatus>>({});
  $effect(() => {
    const paths = ed.drafts.filter((d) => d.enabled && d.kind === 'cratarr').flatMap(chosenOf);
    for (const p of paths) {
      if (statuses[p]) continue;
      folderStatus(p).then(
        (st) => (statuses[p] = st),
        () => {},
      );
    }
  });

  const size = (b: number) => (b >= 1e12 ? `${(b / 1e12).toFixed(1)} TB` : `${Math.round(b / 1e9)} GB`);
  const tracksOf = (n: number | null) => (n ? `${count(n)} ${n === 1 ? 'track' : 'tracks'}` : null);
  /** A folder's line: its tracks, its disk's free space, and read only where that matters. */
  const statusLine = (st: FolderStatus, use: string) =>
    [tracksOf(st.tracks), st.free !== null ? `${size(st.free)} free` : null, !st.writable && use !== 'unused' ? 'read only' : null].filter(Boolean).join(' · ');
  /** Another server's library: its path as the server sees it, and where that is in Cratarr's folders. */
  const mapping = (l: { paths: string[]; folders: string[] }) =>
    l.paths.length && l.paths.join(', ') !== l.folders.join(', ') ? `${l.paths.join(', ')} → ${l.folders.join(', ')}` : l.folders.join(', ');

  /** One library as a row and a page show it: its name, where, and what is wrong with it. */
  type Lib = { d: Draft; id: string; name: string; where: string; problem: string | null; status: string | null; folder: boolean };
  function libsOf(d: Draft): Lib[] {
    if (d.kind === 'cratarr')
      return chosenOf(d).map((path) => {
        const st = statuses[path];
        const u = ed.useOf(d, path);
        return {
          d,
          id: path,
          name: folderName(path),
          where: path,
          problem: st && !st.exists ? 'Not found. Mount it into Cratarr and restart it.' : st && !st.writable && u !== 'unused' ? 'Read only' : null,
          status: st && st.exists ? statusLine(st, u) : null,
          folder: true,
        };
      });
    return (d.view?.server_libraries ?? []).map((l: ServerLibrary) => {
      const u = ed.useOf(d, l.id);
      const problem = l.folders.length
        ? l.read_only && u !== 'unused'
          ? `Cratarr can't write here, so ${u === 'music' ? 'playlists can’t be saved' : 'albums can’t be deleted'}. Mount it writable and give Cratarr's user write access.`
          : null
        : u !== 'unused'
          ? `Cratarr can't see these files.${l.paths.length ? ` Mount ${ed.kindOf(d)?.label ?? d.kind}'s folder ${l.paths.join(', ')} into Cratarr and restart it.` : ''}`
          : null;
      return { d, id: l.id, name: l.name, where: l.folders.length ? mapping(l) : l.paths.join(', '), problem, status: null, folder: false };
    });
  }

  const libThing = (l: Lib) => `lib:${l.d.id}:${l.id}`;
  const open = $derived.by(() => {
    if (!thing?.startsWith('lib:')) return null;
    const rest = thing.slice(4);
    const at = rest.indexOf(':');
    const d = ed.drafts.find((x) => x.id === rest.slice(0, at));
    return d ? (libsOf(d).find((l) => l.id === rest.slice(at + 1)) ?? null) : null;
  });
  nameThing(() => (thing === 'tags' ? 'Tag splitting' : open ? open.name : null));
  // A library gone (removed, or its server's): back to the list.
  $effect(() => {
    if (thing && thing !== 'tags' && !open) backFromThing('libraries');
  });

  // The catalogue's progress, while this part shows.
  $effect(() => {
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        ed.cat = await getCatalogue();
      } catch {
        /* the status line keeps its last value */
      }
      if (!stopped) timer = setTimeout(poll, 5000);
    }
    timer = setTimeout(poll, 5000);
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  });

  // A value typed in its sheet: Names (measured as it is typed, put back on Cancel), the days, the tag rules.
  type Typing = { title: string; value: string; type?: 'text' | 'number'; help?: string; placeholder?: string; lines?: boolean; suffix?: string; save: (v: string) => void };
  let typing = $state<Typing | null>(null);
  let naming = $state<string | null>(null);
  let namingTaken = false;
  let colouring = $state<Lib | null>(null);
  /** The library whose use is being chosen (the choice sheet). */
  let choosingUse = $state<Lib | null>(null);
  let removing = $state<Lib | null>(null);
  const incomingUsed = $derived(ed.drafts.some((d) => d.enabled && d.libraries.incoming.length));
  const lines = (t: string) => t.split('\n').map((l) => l.trim()).filter(Boolean);
  // The server's own settings (lib/serverRuntime), read once here.
  loadRuntime();
  const ANALYSIS: { value: 'keep' | 'all' | 'off'; label: string }[] = [
    { value: 'keep', label: 'New music' },
    { value: 'all', label: 'All' },
    { value: 'off', label: 'Off' },
  ];
  const ANALYSIS_MORE =
    "Cratarr's own analysis of the music: waveforms, tempo, loudness and the visuals' signals.\n\nNew music: what comes in, once half the library has an analysis (the first pass made elsewhere, or all of it here). All: everything, here. Off: none here.";
  const BEAT_THIS_MORE =
    'Beats and downbeats as the Beat This! model hears them, for the visuals: heard here as new music comes in (about 650 MB while it hears). Off, they come from tools/beat_this_pass.py on another computer, or the visuals keep the beat grid of the analysis.';
</script>

{#if open}
  {@const l = open}
  {@const u = ed.useOf(l.d, l.id)}
  {@const pol = ed.policyOf(l.d, l.id)}
  <Group>
    <!-- The row says it, the choice sheet picks it (the owner, 2026-10-25). -->
    <Item name="Use" more={USE_MORE} value={useLabel(u)} onclick={() => (choosingUse = l)} />
    {#if u !== 'unused'}
      <Item name="Albums moved in">
        <Seg label={`Albums moved into ${l.name}`} options={FORMAT_OPTIONS} bind:value={() => pol.format, (v) => ed.setPolicy(l.d, l.id, { format: v })} />
      </Item>
    {/if}
    {#if u === 'incoming'}
      <Item name="Dot colour" swatch={pol.colour ?? ed.palette[0] ?? '#e0a23a'} onclick={() => (colouring = l)} />
    {/if}
  </Group>
  <Group title={l.folder ? 'Folder' : 'Where'}>
    <Item lead={l.problem ? 'warn' : 'on'} name={l.where || l.name} line={l.problem ?? l.status} tone={l.problem ? 'warn' : null} wrap />
  </Group>
  {#if l.folder}
    <Group>
      <Item name="Remove folder" kind="danger" onclick={() => (removing = l)} />
    </Group>
  {/if}
{:else if thing === 'tags'}
  <Group title="Separators" more={TAGS_MORE}>
    <Item
      name="Artist separators"
      line={ed.ruleText.artistSeps || 'None'}
      onclick={() => (typing = { title: 'Artist separators', value: ed.ruleText.artistSeps, help: 'Separated by spaces', save: (v) => (ed.ruleText.artistSeps = v) })}
    />
    <Item
      name="Genre separators"
      line={ed.ruleText.genreSeps || 'None'}
      onclick={() => (typing = { title: 'Genre separators', value: ed.ruleText.genreSeps, help: 'Separated by spaces', save: (v) => (ed.ruleText.genreSeps = v) })}
    />
  </Group>
  <Group title="Never split">
    <Item
      name="Artists"
      line={lines(ed.ruleText.artistKeep).join(' · ') || 'None'}
      onclick={() => (typing = { title: 'Artists never split', value: ed.ruleText.artistKeep, help: 'One per line', placeholder: 'AC/DC', lines: true, save: (v) => (ed.ruleText.artistKeep = v) })}
    />
    <Item
      name="Genres"
      line={lines(ed.ruleText.genreKeep).join(' · ') || 'None'}
      onclick={() => (typing = { title: 'Genres never split', value: ed.ruleText.genreKeep, help: 'One per line', placeholder: 'Funk / Soul', lines: true, save: (v) => (ed.ruleText.genreKeep = v) })}
    />
  </Group>
  <!-- Names given otherwise (0.9.7; the owner, 2026-10-26: decided once, for the library, what arrives and the harvest):
       one name for several, written into the files through Health's fixes. -->
  <Group title="Named otherwise">
    <Item
      name="Genres"
      line={lines(ed.ruleText.genreAliases).join(' · ') || 'None'}
      onclick={() =>
        (typing = {
          title: 'Genres named otherwise',
          value: ed.ruleText.genreAliases,
          help: 'One per line: the names, then → and the one name',
          placeholder: 'Hip-Hop, Hip Hop Music → Hip Hop',
          lines: true,
          save: (v) => (ed.ruleText.genreAliases = v),
        })}
    />
    <Item
      name="Artists"
      line={lines(ed.ruleText.artistAliases).join(' · ') || 'None'}
      onclick={() =>
        (typing = {
          title: 'Artists named otherwise',
          value: ed.ruleText.artistAliases,
          help: 'One per line: the names, then → and the one name',
          placeholder: 'Beatles → The Beatles',
          lines: true,
          save: (v) => (ed.ruleText.artistAliases = v),
        })}
    />
  </Group>
{:else}
  {#if ed.drafts.some((d) => d.enabled && d.view) && !ed.drafts.some((d) => d.enabled && d.libraries.music.length)}
    <Group><Item lead="warn" name="Choose a Music library" /></Group>
  {/if}
  {#each ed.drafts.filter((d) => d.enabled) as d (d)}
    <Group title={serverLabel(d)}>
      {#if d.kind !== 'cratarr' && !d.view}
        <Item name="Check the connection to see this server's libraries" />
      {:else}
        {#each libsOf(d) as l (l.id)}
          {@const u = ed.useOf(d, l.id)}
          {@const colour = u === 'incoming' ? ed.policyOf(d, l.id).colour : null}
          <Item
            lead={colour ?? (libsOf(d).some((x) => ed.useOf(d, x.id) === 'incoming' && ed.policyOf(d, x.id).colour) ? '' : null)}
            name={l.name}
            line={l.problem ?? [useLabel(u), l.where, l.status].filter(Boolean).join(' · ')}
            tone={l.problem ? 'warn' : null}
            onclick={() => openThing('libraries', libThing(l))}
          />
        {:else}
          <Item name={d.kind === 'cratarr' ? 'No folders yet' : 'No music libraries on this server'} />
        {/each}
        {#if d.kind === 'cratarr'}<Item name="Add folder" kind="add" onclick={() => (browsing = d)} />{/if}
      {/if}
    </Group>
  {:else}
    <Group><Item name="Add a server under Connections first" /></Group>
  {/each}

  {#if ed.namingInUse}
    <Group title="Music">
      <Item
        name="Names"
        line={ed.naming}
        tone={ed.namingError ? 'bad' : null}
        wrap
        onclick={() => {
          naming = ed.naming;
          namingTaken = false;
        }}
      />
    </Group>
  {/if}
  {#if incomingUsed}
    <Group title="Incoming">
      <Item
        name="Keep deleted albums"
        value={`${ed.trashDays} ${ed.trashDays === 1 ? 'day' : 'days'}`}
        onclick={() =>
          (typing = {
            title: 'Keep deleted albums',
            value: String(ed.trashDays),
            type: 'number',
            suffix: 'days',
            save: (v) => (ed.trashDays = Math.min(3650, Math.max(1, Math.round(Number(v)) || 1))),
          })}
      />
    </Group>
  {/if}
  <Group title="Catalogue">
    {#if ed.cat}
      {@const c = ed.catalogueLine(ed.cat)}
      <Item
        lead={c.doing ? 'off' : ed.cat.progress.problems.length ? 'warn' : 'on'}
        name={c.parts[0]}
        line={c.doing ? `Working: ${c.doing}…` : c.when ? `Up to date · checked ${c.when}` : 'Starting…'}
        more={c.parts.length > 1 ? c.parts.slice(1).join('\n\n') : null}
      />
      {#each ed.cat.progress.problems as p (p)}<Item lead="bad" name={p} />{/each}
    {:else}
      <Item name="Not available" />
    {/if}
    <Item
      name="Tag splitting"
      line={[`Artists: ${ed.ruleText.artistSeps || 'not split'}`, `Genres: ${ed.ruleText.genreSeps || 'not split'}`].join(' · ')}
      onclick={() => openThing('libraries', 'tags')}
    />
    <!-- The analysis on this server and Beat This! (0.9.8: the environment's until then; lib/serverRuntime). -->
    {#if runtime.value}
      {@const r = runtime.value}
      <Item name="Analysis here" more={ANALYSIS_MORE} line={r.from_env?.analysis ? 'Set by the environment until changed here' : null}>
        <Seg label="Analysis here" options={ANALYSIS} bind:value={() => r.analysis, (v) => void saveRuntime({ analysis: v })} />
      </Item>
      <Item
        name="Beats with Beat This!"
        more={BEAT_THIS_MORE}
        line={r.from_env?.beat_this ? 'Set by the environment until changed here' : 'About 650 MB while it hears'}
        toggle={{ checked: r.beat_this, onchange: (on) => void saveRuntime({ beat_this: on }) }}
      />
      {#if r.restart?.some((x) => x === 'analysis' || x === 'beat_this')}<Item lead="warn" name="Takes effect when Cratarr restarts" />{/if}
    {/if}
    {#if runtime.error}<Item lead="bad" name={runtime.error} />{/if}
  </Group>
{/if}

{#if browsing}
  {@const d = browsing}
  <FolderBrowser chosen={chosenOf(d)} onadd={(p) => addFolder(d, p)} onclose={() => (browsing = null)} />
{/if}
{#if typing}
  {@const t = typing}
  <ValueSheet
    title={t.title}
    value={t.value}
    type={t.type ?? 'text'}
    help={t.help ?? null}
    placeholder={t.placeholder ?? ''}
    lines={t.lines ?? false}
    suffix={t.suffix ?? null}
    min={t.type === 'number' ? 1 : null}
    max={t.type === 'number' ? 3650 : null}
    onsave={(v) => t.save(v)}
    onclose={() => (typing = null)}
  />
{/if}
{#if naming !== null}
  {@const before = naming}
  <ValueSheet
    title="Names"
    value={before}
    allowEmpty={false}
    oninput={(v) => {
      ed.naming = v;
      ed.typedNaming();
    }}
    onsave={(v) => {
      namingTaken = true;
      ed.naming = v;
    }}
    onclose={() => {
      // Cancel puts the names back as they were.
      if (!namingTaken) {
        ed.naming = before;
        ed.typedNaming();
      }
      naming = null;
    }}
  >
    {#snippet extra()}
      {#if ed.namingError}
        <p class="check bad">{ed.namingError}</p>
      {:else if ed.namingCheck}
        <p class="check">
          Matches {count(ed.namingCheck.folders.matched)} of {count(ed.namingCheck.folders.total)} folders and
          {count(ed.namingCheck.files.matched)} of {count(ed.namingCheck.files.total)} files.
        </p>
        {#if ed.namingCheck.example}<code class="check">{ed.namingCheck.example}</code>{/if}
      {/if}
    {/snippet}
  </ValueSheet>
{/if}
{#if choosingUse}
  {@const l = choosingUse}
  <ChoiceSheet title={`Use of ${l.name}`} options={USE_OPTIONS} value={ed.useOf(l.d, l.id)} onchange={(v) => ed.setUse(l.d, l.id, v)} onclose={() => (choosingUse = null)} />
{/if}
{#if colouring}
  {@const l = colouring}
  {@const pol = ed.policyOf(l.d, l.id)}
  <MenuSheet label="Dot" title={`Dot of ${l.name}`} onclose={() => (colouring = null)}>
    <div class="swatches" role="radiogroup" aria-label={`Colour of ${l.name}`}>
      {#each ed.palette as c (c)}
        <button
          class="swatch"
          role="radio"
          aria-checked={pol.colour === c}
          aria-label={c}
          style:background={c}
          onclick={() => {
            ed.setPolicy(l.d, l.id, { colour: c });
            colouring = null;
          }}
        ></button>
      {/each}
      <label class="swatch own" class:on={!!pol.colour && !ed.palette.includes(pol.colour)} style:background={pol.colour && !ed.palette.includes(pol.colour) ? pol.colour : null}>
        <span class="sr">Another colour</span>
        <input type="color" value={pol.colour ?? ed.palette[0] ?? '#e0a23a'} onchange={(e) => ed.setPolicy(l.d, l.id, { colour: e.currentTarget.value })} />
      </label>
    </div>
  </MenuSheet>
{/if}
{#if removing}
  {@const l = removing}
  <ConfirmSheet
    title={`Remove ${l.name}?`}
    text="Its files stay where they are."
    label="Remove"
    onconfirm={() => {
      removeFolder(l.d, l.id);
      backFromThing('libraries');
    }}
    onclose={() => (removing = null)}
  />
{/if}

<style>
  .check {
    margin: 0;
    color: var(--muted);
    font-size: 13px;
    overflow-wrap: anywhere;
  }

  .check.bad {
    color: var(--danger-ink);
  }

  code.check {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  }

  .swatches {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    padding: 4px 24px 20px;
  }

  .swatch {
    position: relative;
    width: 40px;
    height: 40px;
    padding: 0;
    border: 2px solid transparent;
    border-radius: 50%;
    cursor: pointer;
  }

  .swatch[aria-checked='true'],
  .swatch.own.on {
    border-color: var(--text);
    box-shadow: inset 0 0 0 2px var(--surface);
  }

  .swatch.own {
    background: conic-gradient(#e0a23a, #d65c7a, #9b6ad6, #4f8fe0, #3bb39b, #e0a23a);
  }

  .swatch.own input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0;
    cursor: pointer;
  }

  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
</style>
