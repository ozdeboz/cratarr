<script lang="ts">
  // Libraries: each server library's use and, for a used one, its format and dot (decision 0034); Music's names, the
  // trash, the catalogue and tag splitting. Saved with the Save bar.
  import { ed } from './editor.svelte';
  import { folderStatus, getCatalogue, type FolderStatus } from '../api';
  import FolderBrowser from './FolderBrowser.svelte';
  import { count, FORMATS, USES } from './editor.svelte';
  import Seg from '../Seg.svelte';
  import SettingRow from '../SettingRow.svelte';

  // Seg's choices: a use, a format.
  const USE_OPTIONS = USES.map((x) => ({ value: x.id, label: x.label }));
  const FORMAT_OPTIONS = FORMATS.map((f) => ({ value: f.id, label: f.label }));

  const many = $derived(ed.drafts.length > 1);
  type Draft = (typeof ed.drafts)[number];

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
  /** A folder's line: its tracks, its disk's free space, and read only where that matters. */
  const statusLine = (st: FolderStatus, use: string) =>
    [st.tracks ? `${count(st.tracks)} ${st.tracks === 1 ? 'track' : 'tracks'}` : null, st.free !== null ? `${size(st.free)} free` : null, !st.writable && use !== 'unused' ? 'read only' : null]
      .filter(Boolean)
      .join(' · ');
  /** Another server's library: its path as the server sees it, and where that is in Cratarr's folders. */
  const mapping = (l: { paths: string[]; folders: string[] }) =>
    l.paths.length && l.paths.join(', ') !== l.folders.join(', ') ? `${l.paths.join(', ')} → ${l.folders.join(', ')}` : l.folders.join(', ');

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
</script>

  <section aria-labelledby="lib-h">
    <h2 id="lib-h">Libraries</h2>
    <p class="lede">
      <strong>Music</strong>: the playlists and the tracks that can go in them. <strong>Incoming</strong>: new music
      waiting to be heard.
    </p>

    {#snippet controls(d: Draft, id: string, name: string)}
      {@const u = ed.useOf(d, id)}
      <Seg label={`Use of ${name}`} options={USE_OPTIONS} bind:value={() => u, (v) => ed.setUse(d, id, v)} />
      {#if u !== 'unused'}
        {@const pol = ed.policyOf(d, id)}
        <!-- Its own settings (decision 0034), a row each under it (lib/SettingRow). -->
        <div class="policies">
          <SettingRow label="Albums moved in">
            <Seg label={`Albums moved into ${name}`} options={FORMAT_OPTIONS} bind:value={() => pol.format, (v) => ed.setPolicy(d, id, { format: v })} />
          </SettingRow>
          {#if u === 'incoming'}
            <SettingRow label="Dot">
              <div class="swatches" role="radiogroup" aria-label={`Colour of ${name}`}>
                {#each ed.palette as c (c)}
                  <button class="swatch" role="radio" aria-checked={pol.colour === c} aria-label={c} style:background={c} onclick={() => ed.setPolicy(d, id, { colour: c })}></button>
                {/each}
                <label class="swatch own" class:on={!!pol.colour && !ed.palette.includes(pol.colour)} style:background={pol.colour && !ed.palette.includes(pol.colour) ? pol.colour : null}>
                  <span class="sr">Another colour</span>
                  <input type="color" value={pol.colour ?? ed.palette[0] ?? '#e0a23a'} onchange={(e) => ed.setPolicy(d, id, { colour: e.currentTarget.value })} />
                </label>
              </div>
            </SettingRow>
          {/if}
        </div>
      {/if}
    {/snippet}

    {#each ed.drafts.filter((d) => d.enabled) as d (d)}
      <div class="libs-of">
        {#if many}<h3>{ed.kindOf(d)?.label ?? d.kind}<small>{d.id}</small></h3>{/if}
        {#if d.kind === 'cratarr'}
          <!-- Cratarr's own folders (decision 0044): chosen, each with its state; added in the browser. -->
          <ul class="libs">
            {#each chosenOf(d) as path (path)}
              {@const st = statuses[path]}
              {@const u = ed.useOf(d, path)}
              <li>
                <div class="where">
                  <span>{folderName(path)}</span>
                  <code class="hint">{path}</code>
                  {#if st && !st.exists}
                    <span class="hint warn">Not found. Mount it into Cratarr and restart it.</span>
                  {:else if st}
                    <span class="hint" class:warn={!st.writable && u !== 'unused'}>{statusLine(st, u)}</span>
                  {/if}
                </div>
                {@render controls(d, path, folderName(path))}
                <div class="row"><button onclick={() => removeFolder(d, path)}>Remove</button></div>
              </li>
            {:else}
              <li class="hint">No folders yet.</li>
            {/each}
          </ul>
          <div class="row"><button onclick={() => (browsing = d)}>Add folder</button></div>
          {#if browsing === d}
            <FolderBrowser chosen={chosenOf(d)} onadd={(p) => addFolder(d, p)} onclose={() => (browsing = null)} />
          {/if}
        {:else if !d.view}
          <p class="hint">Check the connection to see this server's libraries.</p>
        {:else}
          <ul class="libs">
            {#each d.view.server_libraries as l (l.id)}
              {@const u = ed.useOf(d, l.id)}
              <li>
                <div class="where">
                  <span>{l.name}</span>
                  {#if l.folders.length}
                    <!-- Where the server's path lies in Cratarr's folders (found by itself, decision 0009). -->
                    <code class="hint">{mapping(l)}</code>
                    {#if l.read_only && u !== 'unused'}
                      <span class="hint warn">
                        Cratarr can't write here, so {u === 'music' ? 'playlists can’t be saved' : 'albums can’t be deleted'}. Mount it
                        writable and give Cratarr's user write access.
                      </span>
                    {/if}
                  {:else if u !== 'unused'}
                    <span class="hint warn">
                      Cratarr can't see these files.{#if l.paths.length}
                        Mount {ed.kindOf(d)?.label ?? d.kind}'s folder <code>{l.paths.join(', ')}</code> into Cratarr and restart it.{/if}
                    </span>
                  {/if}
                </div>
                {@render controls(d, l.id, l.name)}
              </li>
            {:else}
              <li class="hint">No music libraries on this server.</li>
            {/each}
          </ul>
        {/if}
      </div>
    {:else}
      <p class="hint">Add a server under Connections first.</p>
    {/each}
    {#if ed.drafts.some((d) => d.enabled && d.view) && !ed.drafts.some((d) => d.enabled && d.libraries.music.length)}
      <p class="hint warn">Choose a Music library.</p>
    {/if}

    {#if ed.namingInUse}
      <div class="sub fields">
        <label>
          <span class="l">Names in Music</span>
          <input type="text" class="mono" bind:value={ed.naming} oninput={ed.typedNaming} spellcheck="false" autocomplete="off" />
          {#if ed.namingError}
            <span class="hint bad">{ed.namingError}</span>
          {:else if ed.namingCheck}
            <span class="hint">
              Matches {count(ed.namingCheck.folders.matched)} of {count(ed.namingCheck.folders.total)} folders and
              {count(ed.namingCheck.files.matched)} of {count(ed.namingCheck.files.total)} files.
            </span>
            {#if ed.namingCheck.example}<code class="hint example">{ed.namingCheck.example}</code>{/if}
          {/if}
        </label>
      </div>
    {/if}

    {#if ed.drafts.some((d) => d.enabled && d.libraries.incoming.length)}
      <div class="sub fields">
        <label>
          <span class="l">Keep deleted albums for</span>
          <span class="days"><input type="number" min="1" max="3650" step="1" bind:value={ed.trashDays} /> days</span>
        </label>
      </div>
    {/if}

    <div class="sub">
      <h3>Catalogue</h3>
      {#if ed.cat}
        {@const line = ed.catalogueLine(ed.cat)}
        <p class="state"><span class="dot" class:ok={!line.doing && ed.cat.progress.problems.length === 0} aria-hidden="true"></span>{line.parts.join(' · ')}</p>
        <p class="hint">{line.doing ? `Working: ${line.doing}…` : line.when ? `Up to date, checked at ${line.when}.` : 'Starting…'}</p>
        {#each ed.cat.progress.problems as p (p)}<p class="hint bad">{p}</p>{/each}
      {:else}
        <p class="hint">Not available.</p>
      {/if}
    </div>

    <details class="sub">
      <summary>Tag splitting</summary>
      <p class="hint">How tags with several artists or genres in one value are read. The files are never changed.</p>
      <div class="fields">
        <label>
          <span class="l">Artist separators</span>
          <input type="text" bind:value={ed.ruleText.artistSeps} spellcheck="false" />
          <span class="hint">Separated by spaces. "&amp;", "," and "feat." are left alone: they are part of credits.</span>
        </label>
        <label>
          <span class="l">Genre separators</span>
          <input type="text" bind:value={ed.ruleText.genreSeps} spellcheck="false" />
        </label>
        <label>
          <span class="l">Artists never split</span>
          <textarea rows="3" bind:value={ed.ruleText.artistKeep} spellcheck="false" placeholder="AC/DC"></textarea>
          <span class="hint">One per line.</span>
        </label>
        <label>
          <span class="l">Genres never split</span>
          <textarea rows="3" bind:value={ed.ruleText.genreKeep} spellcheck="false" placeholder="Funk / Soul"></textarea>
        </label>
      </div>
    </details>
  </section>
