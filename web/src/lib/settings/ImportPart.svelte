<script lang="ts">
  // Import: plays, favourites, ratings and added dates from a server into an account; acts at once, followed while
  // it runs. A group per server: from whom and into which account (each chosen in a sheet), what, and the run with its
  // result (the settings' North Star, 2026-10-19). And the listening history from ListenBrainz (decision 0064, amended):
  // every listen it kept, into the history of the account signed in.
  import { importListenBrainz } from '../api';
  import ChoiceSheet from './ChoiceSheet.svelte';
  import ValueSheet from './ValueSheet.svelte';
  import { ed, IMPORT_WHAT } from './editor.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';

  const MORE = 'Plays, favourites, ratings and added dates from a server, into your account.';

  // While an import runs, its progress.
  $effect(() => {
    if (!ed.importing) return;
    const t = setInterval(ed.followImport, 2000);
    return () => clearInterval(t);
  });

  let choosing = $state<{ source: string; which: 'user' | 'account' } | null>(null);

  // ListenBrainz: whose listens, and the run's result.
  let lbUser = $state('');
  let naming = $state(false);
  let lb = $state<{ running: boolean; done: { listens: number; matched: number; added: number } | null; error: string | null }>({ running: false, done: null, error: null });
  async function runListenBrainz() {
    lb = { running: true, done: null, error: null };
    try {
      lb = { running: false, done: await importListenBrainz(lbUser), error: null };
    } catch (e) {
      lb = { running: false, done: null, error: e instanceof Error ? e.message : String(e) };
    }
  }
</script>

<Group title="From ListenBrainz" more="Every listen ListenBrainz kept, into your listening history.">
  <Item name="User" value={lbUser || 'Choose…'} onclick={() => (naming = true)} />
  {#if lb.running}
    <Item lead="off" name="Importing" />
  {:else}
    <Item
      lead={lb.done ? 'on' : lb.error ? 'bad' : null}
      name={lb.done ? 'Imported' : lb.error ? 'Not imported' : 'Not run yet'}
      line={lb.done ? `${lb.done.matched.toLocaleString()} of ${lb.done.listens.toLocaleString()} listens found · ${lb.done.added.toLocaleString()} new` : lb.error}
      tone={lb.error ? 'bad' : null}
      wrap
    >
      <button class="primary" disabled={!lbUser.trim()} onclick={runListenBrainz}>Import</button>
    </Item>
  {/if}
</Group>
{#if naming}
  <ValueSheet title="ListenBrainz user" value={lbUser} allowEmpty={false} onsave={(v) => {
      lbUser = v.trim();
    }} onclose={() => (naming = false)} />
{/if}

{#if !ed.sub.known || !ed.imp.known}
  <!-- Not read yet: nothing rather than "Create an account" or "No server" first (the owner, 2026-10-24). -->
{:else if !ed.sub.accounts}
  <Group title="Import" more={MORE}><Item lead="bad" name="Could not read the accounts" /></Group>
{:else if !ed.sub.accounts.length}
  <Group title="Import" more={MORE}><Item name="Create an account under Authentication first" /></Group>
{:else}
  {#each ed.imp.sources as src (src.id)}
    {@const c = ed.imp.choice[src.id]}
    {@const job = ed.imp.job?.source === src.id ? ed.imp.job : null}
    <Group title={`From ${ed.serverName(src.id)}`} more={MORE}>
      {#if src.error}
        <Item lead="bad" name={src.error} />
      {:else if c}
        {@const none = !Object.values(c.what).some(Boolean) || !c.user || !c.account}
        <Item name="From" value={src.users.find((u) => u.id === c.user)?.name ?? 'Choose…'} onclick={() => (choosing = { source: src.id, which: 'user' })} />
        <Item name="Into" value={c.account || 'Choose…'} onclick={() => (choosing = { source: src.id, which: 'account' })} />
        {#each IMPORT_WHAT as w (w.key)}
          <Item name={w.label} toggle={{ checked: c.what[w.key], onchange: (on) => (c.what[w.key] = on) }} />
        {/each}
        {#if job && job.finished === null}
          {@const st = ed.stepLine(job)}
          <Item lead="off" name="Importing" line={`${st.label}${st.value ? ` · ${Math.round(st.value * 100)}%` : ''}`} />
        {:else}
          <Item
            lead={job ? (job.error ? 'bad' : 'on') : null}
            name={job ? (job.error ? 'Not imported' : job.preview ? 'Previewed' : 'Imported') : 'Not run yet'}
            line={job ? (job.error ?? ed.importLine(job)) : null}
            tone={job?.error ? 'bad' : null}
            wrap
          >
            <button disabled={ed.importing || none} onclick={() => ed.runImport(src.id, true)}>Preview</button>
            <button class="primary" disabled={ed.importing || none} onclick={() => ed.runImport(src.id, false)}>Import</button>
          </Item>
        {/if}
        {#if job?.note}<Item lead="warn" name={job.note} />{/if}
      {/if}
    </Group>
  {:else}
    <Group title="Import" more={MORE}><Item name="No server to import from" /></Group>
  {/each}
{/if}

{#if choosing}
  {@const ch = choosing}
  {@const src = ed.imp.sources.find((x) => x.id === ch.source)}
  {@const c = ed.imp.choice[ch.source]}
  {#if src && c}
    <ChoiceSheet
      title={ch.which === 'user' ? 'From' : 'Into'}
      options={ch.which === 'user' ? src.users.map((u) => ({ value: u.id, label: u.name })) : ed.imp.accounts.map((a) => ({ value: a, label: a }))}
      value={ch.which === 'user' ? c.user : c.account}
      onchange={(v) => (ch.which === 'user' ? (c.user = v) : (c.account = v))}
      onclose={() => (choosing = null)}
    />
  {/if}
{/if}
