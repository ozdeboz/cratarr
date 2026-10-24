<script lang="ts">
  // Listening services (decision 0068; the owner, 2026-10-24: in Connections, each person their own): ListenBrainz and
  // Last.fm, to which every play Cratarr counts is sent (and what starts, as now playing). The signed-in account's only;
  // acts at once. ListenBrainz connects with the user token from its settings page; Last.fm with the API key and shared
  // secret made at last.fm/api, then its own sign-in page, which comes back here.
  import { forgetScrobble, getScrobbles, scrobbleLastFm, scrobbleListenBrainz, type ScrobbleTarget } from '../api';
  import ConfirmSheet from './ConfirmSheet.svelte';
  import FormSheet from './FormSheet.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';

  const MORE =
    "Every play Cratarr counts, from any of your players or Subsonic apps, sent to the services you connect, and what starts as now playing. Yours only. Imported listens are never sent back.\n\nA service that is down gets them later; one that refuses the login stops until you connect it again.";
  const NAMES = { listenbrainz: 'ListenBrainz', lastfm: 'Last.fm' } as const;
  type Service = keyof typeof NAMES;

  let targets = $state<ScrobbleTarget[] | null>(null);
  let error = $state<string | null>(null);
  const load = () =>
    getScrobbles().then(
      (r) => (targets = r.targets),
      () => (targets = []),
    );
  load();
  // Back from Last.fm's sign-in with what went wrong, if anything.
  const back = new URLSearchParams(location.hash.split('?')[1] ?? '').get('scrobble');
  if (back) error = back;

  const of = (s: Service) => targets?.find((t) => t.target === s) ?? null;
  function line(t: ScrobbleTarget): { lead: string; line: string; bad: boolean } {
    if (t.broken) return { lead: 'bad', line: t.broken, bad: true };
    const waiting = t.queued ? ` · ${t.queued.toLocaleString()} waiting${t.waiting ? `: ${t.waiting}` : ''}` : '';
    return { lead: 'on', line: `Sending as ${t.name}${waiting}`, bad: false };
  }

  // Connecting: ListenBrainz's token, or Last.fm's key and secret (then its sign-in).
  let connecting = $state<Service | null>(null);
  let token = $state('');
  let apiKey = $state('');
  let apiSecret = $state('');
  let busy = $state(false);
  let forgetting = $state<Service | null>(null);
  async function connect() {
    error = null;
    busy = true;
    try {
      if (connecting === 'listenbrainz') {
        await scrobbleListenBrainz(token.trim());
        token = '';
        connecting = null;
        load();
      } else if (connecting === 'lastfm') {
        const { url } = await scrobbleLastFm(apiKey.trim(), apiSecret.trim(), `${location.origin}/api/scrobble/lastfm/callback`);
        location.assign(url);
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }
  async function forget(s: Service) {
    try {
      await forgetScrobble(s);
      load();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
</script>

<Group title="Listening services" more={MORE}>
  {#each Object.entries(NAMES) as [id, name] (id)}
    {@const s = id as Service}
    {@const t = of(s)}
    {#if t}
      {@const l = line(t)}
      <Item lead={l.lead} name={name} line={l.line} tone={l.bad ? 'bad' : null} wrap>
        <button onclick={() => ((error = null), (connecting = s))}>{t.broken ? 'Connect again' : 'Change'}</button>
        <button onclick={() => (forgetting = s)}>Disconnect</button>
      </Item>
    {:else if targets}
      <Item name={name} line="Not connected" kind="add" onclick={() => ((error = null), (connecting = s))} />
    {/if}
  {/each}
  {#if error && !connecting}<Item lead="bad" name={error} wrap />{/if}
</Group>

{#if connecting === 'listenbrainz'}
  <FormSheet title="ListenBrainz" submitLabel="Connect" canSubmit={!!token.trim()} {busy} {error} onsubmit={connect} onclose={() => ((connecting = null), (error = null))}>
    <!-- svelte-ignore a11y_autofocus -->
    <label>User token<input type="password" bind:value={token} autocomplete="off" spellcheck="false" autofocus /></label>
    <p class="help">On listenbrainz.org, under your name › Settings.</p>
  </FormSheet>
{:else if connecting === 'lastfm'}
  <FormSheet title="Last.fm" submitLabel="Sign in at Last.fm" canSubmit={!!apiKey.trim() && !!apiSecret.trim()} {busy} {error} onsubmit={connect} onclose={() => ((connecting = null), (error = null))}>
    <!-- svelte-ignore a11y_autofocus -->
    <label>API key<input type="text" bind:value={apiKey} autocomplete="off" spellcheck="false" autofocus /></label>
    <label>Shared secret<input type="password" bind:value={apiSecret} autocomplete="off" spellcheck="false" /></label>
    <p class="help">Make both at last.fm/api/account/create (any name will do). Then Last.fm asks you to allow Cratarr, and brings you back here. In the Android app, connect from a browser instead.</p>
  </FormSheet>
{/if}
{#if forgetting}
  {@const s = forgetting}
  <ConfirmSheet
    title={`Disconnect ${NAMES[s]}?`}
    text="Plays are no longer sent there; those still waiting are dropped."
    label="Disconnect"
    onconfirm={() => forget(s)}
    onclose={() => (forgetting = null)}
  />
{/if}

<style>
  .help {
    margin: 0;
    color: var(--muted);
    font-size: 13px;
  }
</style>
