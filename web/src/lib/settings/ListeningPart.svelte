<script lang="ts">
  // Listening services (decision 0068; the owner, 2026-10-24: in Connections, each person their own, managed as every
  // connection is): ListenBrainz and Last.fm, to which every play Cratarr counts is sent (and what starts, as now
  // playing). In the list, a row each with its state, opening its own page (`thing`: `listenbrainz` or `lastfm`): its
  // state, its fields as rows, and Disconnect at the foot. The signed-in account's only; acts at once. ListenBrainz
  // connects with the user token from its settings page; Last.fm with the API key and shared secret made at
  // last.fm/api, then its own sign-in page, which comes back to this page.
  import type { SettingsSection } from '../settingsSections.svelte';
  import { forgetScrobble, getScrobbles, scrobbleLastFm, scrobbleListenBrainz, setScrobbleOn, type ScrobbleTarget } from '../api';
  import { backFromThing, openThing } from '../panel.svelte';
  import { nameThing } from '../settingsSections.svelte';
  import ConfirmSheet from './ConfirmSheet.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import ValueSheet from './ValueSheet.svelte';

  /** `own`: the part alone, naming its pages itself; `part`: the Settings part it is in (Tracking, since 0.9.5). */
  let { thing = null, own = false, part = 'tracking' }: { thing?: string | null; own?: boolean; part?: SettingsSection } = $props();

  const MORE =
    "Every play Cratarr counts, from any of your players or Subsonic apps, sent to the services you connect, and what starts as now playing. Yours only. Imported listens are never sent back.\n\nA service that is down gets them later; one that refuses the login stops until you connect it again.";
  const NAMES = { listenbrainz: 'ListenBrainz', lastfm: 'Last.fm' } as const;
  type Service = keyof typeof NAMES;
  const service = $derived(thing === 'listenbrainz' || thing === 'lastfm' ? (thing as Service) : null);
  // svelte-ignore state_referenced_locally
  if (own) nameThing(() => (service ? NAMES[service] : null));

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
  function stateOf(t: ScrobbleTarget | null): { lead: string; line: string; bad: boolean } {
    if (!t) return { lead: 'off', line: 'Not connected', bad: false };
    if (!t.on) return { lead: 'off', line: `Off · ${t.name}`, bad: false };
    if (t.broken) return { lead: 'bad', line: t.broken, bad: true };
    const waiting = t.queued ? ` · ${t.queued.toLocaleString()} waiting${t.waiting ? `: ${t.waiting}` : ''}` : '';
    return { lead: 'on', line: `Sending as ${t.name}${waiting}`, bad: false };
  }

  const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
  // ListenBrainz: its token, taken (and checked) when given.
  async function giveToken(token: string) {
    error = null;
    try {
      await scrobbleListenBrainz(token.trim());
      load();
    } catch (e) {
      error = said(e);
      throw e;
    }
  }
  // Last.fm: the key and secret typed here, then its sign-in.
  let apiKey = $state('');
  let apiSecret = $state('');
  let signingIn = $state(false);
  async function signIn() {
    error = null;
    signingIn = true;
    try {
      // In the Android app, Last.fm opens in the browser, and its way back ends in the app (cratarr://).
      const inApp = navigator.userAgent.includes('CratarrApp/');
      const { url } = await scrobbleLastFm(apiKey.trim(), apiSecret.trim(), `${location.origin}/api/scrobble/lastfm/callback`, inApp);
      location.assign(url);
    } catch (e) {
      error = said(e);
      signingIn = false;
    }
  }

  // Switched off: nothing is sent and nothing queued meanwhile; the sign-in stays.
  async function switchOn(s: Service, on: boolean) {
    error = null;
    targets = targets?.map((t) => (t.target === s ? { ...t, on } : t)) ?? null;
    try {
      await setScrobbleOn(s, on);
    } catch (e) {
      error = said(e);
    }
    load();
  }

  let typing = $state<'token' | 'key' | 'secret' | null>(null);
  let forgetting = $state(false);
  async function forget(s: Service) {
    try {
      await forgetScrobble(s);
      await load();
      backFromThing(part);
    } catch (e) {
      error = said(e);
    }
  }
</script>

{#if !service}
  <Group title="Listening services" more={MORE}>
    {#each Object.entries(NAMES) as [id, name] (id)}
      {@const s = id as Service}
      {@const t = of(s)}
      {#if t}
        {@const st = stateOf(t)}
        <Item lead={st.lead} {name} line={st.line} tone={st.bad ? 'bad' : null} onclick={() => openThing(part, s)} />
      {:else if targets}
        <Item name={`Add ${name}`} kind="add" onclick={() => openThing(part, s)} />
      {/if}
    {/each}
  </Group>
{:else}
  {@const t = of(service)}
  {@const st = stateOf(t)}
  {#if t}
    {@const s = service}
    <Group>
      <Item
        name={`Use ${NAMES[s]}`}
        line={t.on ? null : 'Nothing is sent meanwhile, and your sign-in stays'}
        toggle={{ checked: t.on, onchange: (on) => switchOn(s, on) }}
      />
    </Group>
  {/if}
  <Group title="Connection" more={MORE}>
    <Item lead={st.lead} name={t && !t.broken ? 'Connected' : 'Not connected'} line={t ? st.line : null} tone={st.bad ? 'bad' : null} wrap />
    {#if error}<Item lead="bad" name={error} wrap />{/if}
  </Group>
  {#if service === 'listenbrainz'}
    <Group title="Sign-in">
      <Item
        name="User token"
        line={t ? 'Given' : 'On listenbrainz.org, under your name › Settings'}
        tone={t ? null : 'warn'}
        onclick={() => (typing = 'token')}
      />
    </Group>
  {:else}
    <Group title="Sign-in" more="Make both at last.fm/api/account/create (any name will do). Then Last.fm asks you to allow Cratarr, and brings you back here.">
      <Item name="API key" line={apiKey ? 'Given' : t ? 'Kept' : 'Not given'} tone={apiKey || t ? null : 'warn'} onclick={() => (typing = 'key')} />
      <Item name="Shared secret" line={apiSecret ? 'Given' : t ? 'Kept' : 'Not given'} tone={apiSecret || t ? null : 'warn'} onclick={() => (typing = 'secret')} />
      <Item name={t && !t.broken ? 'Signed in at Last.fm' : 'Not signed in at Last.fm'} line={apiKey && apiSecret ? null : 'Give the key and the secret first'}>
        <button class="primary" disabled={!apiKey.trim() || !apiSecret.trim() || signingIn} aria-busy={signingIn} onclick={signIn}>Sign in</button>
      </Item>
    </Group>
  {/if}
  {#if t}
    <Group>
      <Item name={`Disconnect ${NAMES[service]}`} kind="danger" onclick={() => (forgetting = true)} />
    </Group>
  {/if}

  {#if typing === 'token'}
    <ValueSheet title="User token" type="password" value="" help="On listenbrainz.org, under your name › Settings." allowEmpty={false} onsave={giveToken} onclose={() => (typing = null)} />
  {:else if typing === 'key'}
    <ValueSheet title="API key" value={apiKey} help="Made at last.fm/api/account/create." allowEmpty={false} onsave={(v) => void (apiKey = v)} onclose={() => (typing = null)} />
  {:else if typing === 'secret'}
    <ValueSheet title="Shared secret" type="password" value="" help="Shown beside the API key at last.fm/api/accounts." allowEmpty={false} onsave={(v) => void (apiSecret = v)} onclose={() => (typing = null)} />
  {/if}
  {#if forgetting}
    {@const s = service}
    <ConfirmSheet
      title={`Disconnect ${NAMES[s]}?`}
      text="Plays are no longer sent there; those still waiting are dropped."
      label="Disconnect"
      onconfirm={() => forget(s)}
      onclose={() => (forgetting = false)}
    />
  {/if}
{/if}
