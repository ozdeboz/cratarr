<script lang="ts">
  // Libraries: each server library's use and, for a used one, its format and dot (decision 0034); Music's names, the
  // trash, the catalogue and tag splitting. Saved with the Save bar.
  import { ed } from './editor.svelte';
  import { getCatalogue } from '../api';
  import { count, FORMATS, USES } from './editor.svelte';

  const many = $derived(ed.drafts.length > 1);

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

    {#each ed.drafts.filter((d) => d.enabled) as d (d)}
      <div class="libs-of">
        {#if many}<h3>{ed.kindOf(d)?.label ?? d.kind}<small>{d.id}</small></h3>{/if}
        {#if !d.view}
          <p class="hint">Check the connection to see this server's libraries.</p>
        {:else}
          <ul class="libs">
            {#each d.view.server_libraries as l (l.id)}
              {@const u = ed.useOf(d, l.id)}
              <li>
                <div class="where">
                  <span>{l.name}</span>
                  {#if l.folders.length}
                    <code class="hint">{l.folders.join(', ')}</code>
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
                <div class="seg" role="group" aria-label={`Use of ${l.name}`}>
                  {#each USES as x (x.id)}
                    <button aria-pressed={u === x.id} onclick={() => ed.setUse(d, l.id, x.id)}>{x.label}</button>
                  {/each}
                </div>
                {#if u !== 'unused'}
                  {@const pol = ed.policyOf(d, l.id)}
                  <div class="policy">
                    <span class="hint">Albums moved in</span>
                    <div class="seg" role="group" aria-label={`Albums moved into ${l.name}`}>
                      {#each FORMATS as f (f.id)}
                        <button aria-pressed={pol.format === f.id} onclick={() => ed.setPolicy(d, l.id, { format: f.id })}>{f.label}</button>
                      {/each}
                    </div>
                  </div>
                  {#if u === 'incoming'}
                    <div class="policy">
                      <span class="hint">Dot</span>
                      <div class="swatches" role="radiogroup" aria-label={`Colour of ${l.name}`}>
                        {#each ed.palette as c (c)}
                          <button
                            class="swatch"
                            role="radio"
                            aria-checked={pol.colour === c}
                            aria-label={c}
                            style:background={c}
                            onclick={() => ed.setPolicy(d, l.id, { colour: c })}
                          ></button>
                        {/each}
                        <label class="swatch own" class:on={!!pol.colour && !ed.palette.includes(pol.colour)} style:background={pol.colour && !ed.palette.includes(pol.colour) ? pol.colour : null}>
                          <span class="sr">Another colour</span>
                          <input type="color" value={pol.colour ?? ed.palette[0] ?? '#e0a23a'} onchange={(e) => ed.setPolicy(d, l.id, { colour: e.currentTarget.value })} />
                        </label>
                      </div>
                    </div>
                  {/if}
                {/if}
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
