<script lang="ts">
  // Import: plays, favourites, ratings and added dates from a server into an account; acts at once, followed while
  // it runs.
  import { ed } from './editor.svelte';
  import Switch from '../Switch.svelte';
  import Meter from '../Meter.svelte';
  import { IMPORT_WHAT } from './editor.svelte';

  // While an import runs, its progress.
  $effect(() => {
    if (!ed.importing) return;
    const t = setInterval(ed.followImport, 2000);
    return () => clearInterval(t);
  });
</script>

  <section aria-labelledby="import-h">
    <h2 id="import-h">Import</h2>
    <p class="lede">Plays, favourites, ratings and added dates from a server, into your account.</p>
    {#if !ed.sub.accounts?.length}
      <p class="hint">Create an account under Authentication first.</p>
    {:else}
      {#each ed.imp.sources as src (src.id)}
        {@const c = ed.imp.choice[src.id]}
        {@const job = ed.imp.job?.source === src.id ? ed.imp.job : null}
        <div class="sub import">
          <h3>Import from {ed.serverName(src.id)}</h3>
          {#if src.error}
            <p class="hint bad">{src.error}</p>
          {:else if c}
            {@const none = !Object.values(c.what).some(Boolean) || !c.user || !c.account}
            <div class="fields pair">
              <label>
                <span class="l">From</span>
                <select bind:value={c.user}>
                  {#each src.users as u (u.id)}<option value={u.id}>{u.name}</option>{/each}
                </select>
              </label>
              <label>
                <span class="l">Into</span>
                <select bind:value={c.account}>
                  {#each ed.imp.accounts as a (a)}<option value={a}>{a}</option>{/each}
                </select>
              </label>
            </div>
            <div class="checks">
              {#each IMPORT_WHAT as w (w.key)}
                <Switch label={w.label} checked={c.what[w.key]} onchange={(on) => (c.what[w.key] = on)} />
              {/each}
            </div>
            <div class="row">
              <button disabled={ed.importing || none} onclick={() => ed.runImport(src.id, true)}>Preview</button>
              <button class="primary" disabled={ed.importing || none} onclick={() => ed.runImport(src.id, false)}>Import</button>
            </div>
            {#if job}
              {#if job.finished === null}
                {@const st = ed.stepLine(job)}
                <Meter label={st.label} value={st.value} />
              {:else if job.error}
                <p class="hint bad">{job.error}</p>
              {:else}
                <p class="state"><span class="dot ok" aria-hidden="true"></span>{ed.importLine(job)}</p>
              {/if}
              {#if job.note}<p class="hint warn">{job.note}</p>{/if}
            {/if}
          {/if}
        </div>
      {:else}
        <p class="hint">No server to import from.</p>
      {/each}
    {/if}
  </section>
