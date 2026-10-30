<script lang="ts">
  // Users (decision 0051; the owner: "a user panel with a permissions section ... It's probably how Jellyfin does it
  // too"; amended 2026-10-19: "LLDAP shouldn't be the only way for us to manage users"): every account a row, its role
  // and whether it is active in a line, and "+ Add user" last; its own page behind it (`thing`: the name, the settings'
  // North Star, 2026-10-19). Two kinds, one list:
  //   - Cratarr's own accounts, made here: their role, a new password, and Remove (switched off, their data kept;
  //     Restore switches them on again), never one's own and never the last active administrator's;
  //   - a directory's accounts: their name and role set there, read-only here; one who left its music group is
  //     deactivated, their data kept.
  // Both: what they may do beyond listening, and their devices and sign-ins, each to end. Its administrators' only.
  // Acts at once; the server says why it refuses, in a line.
  import {
    createUser,
    endSession,
    getUsers,
    removable,
    removeDevice,
    setPermissions,
    setUserActive,
    setUserPassword,
    setUserRole,
    type OwnDevice,
    type Permissions,
    type User,
    type UserSession,
  } from '../api';
  import { me } from '../me.svelte';
  import { backFromThing, openThing } from '../panel.svelte';
  import PageLoading from '../PageLoading.svelte';
  import Switch from '../Switch.svelte';
  import { nameThing } from '../settingsSections.svelte';
  import { toast } from '../toast.svelte';
  import ConfirmSheet from './ConfirmSheet.svelte';
  import FormSheet from './FormSheet.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import ValueSheet from './ValueSheet.svelte';
  import { when } from './when';

  let { thing = null }: { thing?: string | null } = $props();

  // Downloading and changing playlists bind OpenSubsonic apps; sharing playlists binds Cratarr too. Public share links
  // (`links`) wait until links are offered.
  const PERMISSIONS: { key: keyof Permissions; label: string }[] = [
    { key: 'offline', label: 'Download in apps' },
    { key: 'playlists', label: 'Change playlists in apps' },
    { key: 'share', label: 'Share playlists' },
    { key: 'request', label: 'Request albums' },
    { key: 'auto_approve', label: 'Approve requests automatically' },
    { key: 'manage_requests', label: 'Answer requests' },
  ];
  let users = $state<User[] | null>(null);
  const load = () =>
    getUsers().then(
      (u) => (users = u),
      (e) => {
        users = [];
        toast.fail(e, 'Could not read the users');
      },
    );
  load();

  const open = $derived(thing && users ? (users.find((u) => u.name === thing) ?? null) : null);
  nameThing(() => (open ? open.name : null));
  $effect(() => {
    if (thing && users && !open) backFromThing('users');
  });

  /** A user's line: role, a directory's account, and inactive or how many devices. */
  const lineOf = (u: User) =>
    [
      u.admin ? 'Administrator' : 'User',
      u.directory ? 'directory' : null,
      !u.active ? 'inactive' : u.devices.length ? `${u.devices.length} ${u.devices.length === 1 ? 'device' : 'devices'}` : null,
    ]
      .filter(Boolean)
      .join(' · ');
  const isMe = (u: User) => !!me.name && u.name === me.name;

  async function permit(u: User, key: keyof Permissions, on: boolean) {
    const was = u.permissions[key];
    u.permissions[key] = on;
    try {
      await setPermissions(u.name, { ...u.permissions });
    } catch (e) {
      u.permissions[key] = was;
      toast.fail(e, 'Could not change it');
    }
  }

  async function end(u: User, s: UserSession) {
    try {
      await endSession(u.name, s.id);
      u.sessions = u.sessions.filter((x) => x.id !== s.id);
    } catch (e) {
      toast.fail(e, 'Could not sign it out');
    }
  }

  async function remove(u: User, d: OwnDevice) {
    try {
      await removeDevice(u.name, d.id);
      u.devices = u.devices.filter((x) => x.id !== d.id);
    } catch (e) {
      toast.fail(e, 'Could not remove it');
    }
  }

  // A Cratarr account's role, password, and Remove or Restore.
  async function role(u: User, admin: boolean) {
    const was = u.admin;
    u.admin = admin;
    try {
      await setUserRole(u.name, admin);
    } catch (e) {
      u.admin = was;
      toast.fail(e, 'Could not change the role');
    }
  }
  async function password(u: User, p: string) {
    try {
      await setUserPassword(u.name, p);
      u.sessions = [];
    } catch (e) {
      toast.fail(e, 'Could not set the password');
      throw e;
    }
  }
  async function activate(u: User, active: boolean) {
    try {
      await setUserActive(u.name, active);
      await load();
    } catch (e) {
      toast.fail(e, active ? 'Could not restore it' : 'Could not remove it');
    }
  }
  let passwording = $state<User | null>(null);
  let removing = $state<User | null>(null);

  // A new Cratarr account: its name, a password, and its role; the first one there is administers.
  const firstOwn = $derived(!!users && !users.some((u) => !u.directory));
  let adding = $state(false);
  let newName = $state('');
  let newPassword = $state('');
  let newRole = $state<'user' | 'admin'>('user');
  let busy = $state(false);
  let error = $state<string | null>(null);
  async function add() {
    busy = true;
    error = null;
    try {
      const name = newName.trim();
      await createUser(name, newPassword, firstOwn || newRole === 'admin');
      adding = false;
      newName = '';
      newPassword = '';
      newRole = 'user';
      await load();
      openThing('users', name);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }
</script>

{#if users === null}
  <PageLoading />
{:else if open}
  {@const u = open}
  {#if u.directory}
    <Group>
      <Item name={u.admin ? 'Administrator' : 'User'} line={u.active ? 'Managed in the directory' : 'Inactive: no longer in the music group'} tone={u.active ? null : 'warn'} />
    </Group>
  {:else}
    <Group>
      {#if isMe(u) || !u.active}
        <Item name="Role" value={u.admin ? 'Administrator' : 'User'} line={u.active ? null : 'Removed: switched off, their data kept'} tone={u.active ? null : 'warn'} />
      {:else}
        <!-- A yes or no (the owner, 2026-10-25): administers or not. -->
        <Item name="Administrator" toggle={{ checked: u.admin, onchange: (on) => role(u, on) }} />
      {/if}
      {#if u.active}<Item name="Password" value="Set a new one" onclick={() => (passwording = u)} />{/if}
    </Group>
  {/if}
  {#if !u.admin}
    <Group title="Permissions" more="Apps: OpenSubsonic apps. Administrators may do everything.">
      {#each PERMISSIONS as p (p.key)}
        <Item name={p.label} disabled={!u.active} toggle={{ checked: u.permissions[p.key], onchange: (on) => permit(u, p.key, on) }} />
      {/each}
    </Group>
  {/if}
  {#if u.devices.length}
    <Group title="Devices">
      {#each u.devices as d (d.id)}
        <Item lead={d.online ? 'on' : 'off'} name={d.name} line={`${d.kind} · ${d.online ? 'online' : d.last_seen ? when(d.last_seen) : 'offline'}`}>
          {#if removable(d)}<button onclick={() => remove(u, d)}>Remove</button>{/if}
        </Item>
      {/each}
    </Group>
  {/if}
  {#if u.sessions.length}
    <Group title="Signed in">
      {#each u.sessions as s (s.id)}
        <Item name={s.device} line={when(s.last_used)}>
          <button onclick={() => end(u, s)}>Sign out</button>
        </Item>
      {/each}
    </Group>
  {/if}
  {#if !u.directory && !isMe(u)}
    <Group>
      {#if u.active}
        <Item name={`Remove ${u.name}`} kind="danger" onclick={() => (removing = u)} />
      {:else}
        <Item name={`Restore ${u.name}`} kind="add" onclick={() => activate(u, true)} />
      {/if}
    </Group>
  {/if}
{:else if !thing}
  <Group>
    {#each users as u (u.name)}
      <Item name={u.name} line={lineOf(u)} tone={u.active ? null : 'warn'} onclick={() => openThing('users', u.name)} />
    {/each}
    <Item name="Add user" kind="add" onclick={() => ((error = null), (adding = true))} />
  </Group>
{/if}

{#if adding}
  <FormSheet
    title="Add user"
    submitLabel="Add"
    canSubmit={!!newName.trim() && !!newPassword}
    {busy}
    {error}
    onsubmit={add}
    onclose={() => ((adding = false), (newPassword = ''), (error = null))}
  >
    <!-- svelte-ignore a11y_autofocus -->
    <label>Name<input type="text" bind:value={newName} autocomplete="off" autocapitalize="off" spellcheck="false" maxlength="64" autofocus /></label>
    <label>Password<input type="password" bind:value={newPassword} autocomplete="new-password" /></label>
    {#if firstOwn}
      <p class="first">The first account administers.</p>
    {:else}
      <Switch card label="Administrator" checked={newRole === 'admin'} onchange={(on) => (newRole = on ? 'admin' : 'user')} />
    {/if}
  </FormSheet>
{/if}
{#if passwording}
  {@const u = passwording}
  <ValueSheet title={`Password of ${u.name}`} type="password" value="" placeholder="New password" allowEmpty={false} saveLabel="Save" onsave={(p) => password(u, p)} onclose={() => (passwording = null)} />
{/if}
{#if removing}
  {@const u = removing}
  <ConfirmSheet title={`Remove ${u.name}?`} text="Switched off; their playlists and history are kept." label="Remove" onconfirm={() => activate(u, false)} onclose={() => (removing = null)} />
{/if}

<style>
  .first {
    margin: 0;
    color: var(--muted);
    font-size: 13px;
  }
</style>
