<script lang="ts">
  // Servers (decision 0098, the owner, 2026-11-03: "Settings › This device › Servers"): Cratarr's first, then each
  // Jellyfin or Subsonic server this device is signed in to, each a mode of its own; its own page behind its row
  // (lib/settings/ServerPage). Adding one: its kind, then its address and its own sign-in, which the person fills in
  // (Jellyfin's Quick Connect, a code approved in another of its apps, where the server has it on). Only in the apps.
  import { appInfo } from '../app.svelte';
  import { openThing } from '../panel.svelte';
  import { KIND_NAMES, keepServer, servers, signIn, startQuickConnect, type ServerKind } from '../servers.svelte';
  import Seg from '../Seg.svelte';
  import ChoiceSheet from './ChoiceSheet.svelte';
  import FormSheet from './FormSheet.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';

  const host = (url: string | null) => {
    try {
      return url ? new URL(url).host : null;
    } catch {
      return url;
    }
  };

  type How = 'password' | 'quick' | 'key';
  type Form = { kind: ServerKind; url: string; user: string; secret: string; how: How; busy: boolean; error: string | null; code: string | null; stop: (() => void) | null };
  let choosing = $state(false);
  let form = $state<Form | null>(null);

  function start(kind: ServerKind) {
    choosing = false;
    form = { kind, url: '', user: '', secret: '', how: 'password', busy: false, error: null, code: null, stop: null };
  }
  function close() {
    form?.stop?.();
    form = null;
  }
  const said = (e: unknown, url: string) => (e instanceof TypeError ? `Can't reach ${host(url) ?? 'the server'}` : e instanceof Error ? e.message : 'The sign-in failed');
  const ready = (f: Form) => !!f.url.trim() && (f.how === 'quick' || (!!f.secret && (f.how === 'key' || !!f.user.trim())));

  async function submit(f: Form) {
    f.busy = true;
    f.error = null;
    try {
      if (f.how === 'quick') {
        const q = await startQuickConnect(f.url);
        if (!q) throw new Error('Quick Connect is off on this server');
        f.code = q.code;
        f.stop = q.stop;
        keepServer(await q.wait());
      } else {
        keepServer(await signIn(f.kind, f.url, f.user, f.secret, f.how === 'key'));
      }
      form = null;
    } catch (e) {
      if (form === f && !(e instanceof Error && e.message === 'Stopped')) f.error = said(e, f.url);
      f.code = null;
      f.stop = null;
    } finally {
      f.busy = false;
    }
  }
</script>

<Group title="Servers" more="Each is a mode of its own, beside Cratarr's and Local: its music, its playlists and its lists, played on this device. Its sign-in is kept on this device only.">
  <Item name="Cratarr" line={host(appInfo.inUse)} />
  {#each servers.list ?? [] as s (s.id)}
    <Item name={s.name} line={`${KIND_NAMES[s.kind]} · ${s.user}`} onclick={() => openThing('device', s.id)} />
  {/each}
  <Item name="Add a server" kind="add" onclick={() => (choosing = true)} />
</Group>

{#if choosing}
  <ChoiceSheet
    title="Add a server"
    options={[
      { value: 'jellyfin', label: 'Jellyfin' },
      { value: 'subsonic', label: 'Subsonic (Navidrome, Gonic, Airsonic…)' },
    ]}
    value={''}
    onchange={(k) => start(k as ServerKind)}
    onclose={() => (choosing = false)}
  />
{/if}

{#if form}
  {@const f = form}
  <FormSheet title={KIND_NAMES[f.kind]} submitLabel={f.code ? 'Waiting…' : f.how === 'quick' ? 'Get a code' : 'Sign in'} canSubmit={ready(f) && !f.code} busy={f.busy && !f.code} error={f.error} onsubmit={() => submit(f)} onclose={close}>
    <!-- svelte-ignore a11y_autofocus -->
    <label>Address<input type="url" bind:value={f.url} placeholder={f.kind === 'jellyfin' ? 'http://192.168.1.10:8096' : 'http://192.168.1.10:4533'} autocomplete="off" spellcheck="false" autofocus disabled={!!f.code} /></label>
    <Seg
      label="Sign in with"
      options={f.kind === 'jellyfin'
        ? [
            { value: 'password', label: 'Password' },
            { value: 'quick', label: 'Quick Connect' },
          ]
        : [
            { value: 'password', label: 'Password' },
            { value: 'key', label: 'API key' },
          ]}
      bind:value={f.how}
      fill
    />
    {#if f.how === 'password'}
      <label>User<input bind:value={f.user} autocomplete="username" spellcheck="false" /></label>
      <label>Password<input type="password" bind:value={f.secret} autocomplete="current-password" /></label>
    {:else if f.how === 'key'}
      <label>API key<input type="password" bind:value={f.secret} autocomplete="off" /></label>
    {:else if f.code}
      <p class="code" role="status">Enter <strong>{f.code}</strong> in Quick Connect in another of this Jellyfin's apps, where you are signed in.</p>
    {/if}
  </FormSheet>
{/if}

<style>
  .code {
    margin: 4px 0;
    color: var(--text);
    line-height: 1.4;
  }

  .code strong {
    font-size: 1.3em;
    letter-spacing: 0.12em;
  }
</style>
