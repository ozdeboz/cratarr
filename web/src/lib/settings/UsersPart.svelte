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
  // Their own music is here too: the one place an administrator deletes in it (the owner, 2026-11-04: "not anywhere else
  // than in a control panel"); once removed, it moves into a main library or goes altogether (decision 0101 point 7).
  // Acts at once; the server says why it refuses, in a line.
  import { library, readLibrary } from '../albums.svelte';
  import { librariesOf } from '../copiesRule';
  import {
    createUser,
    deleteOwnAlbum,
    endSession,
    getUsers,
    moveAlbum,
    removable,
    removeDevice,
    setPermissions,
    setUserActive,
    setUserPassword,
    setUserRole,
    type LibraryInfo,
    type ListedAlbum,
    type OwnDevice,
    type Permissions,
    type User,
    type UserSession,
  } from '../api';
  import { plural } from '../insights.svelte';
  import { fewAtATime } from '../many.svelte';
  import { me } from '../me.svelte';
  import Notice from '../Notice.svelte';
  import { backFromThing, openThing } from '../panel.svelte';
  import PageLoading from '../PageLoading.svelte';
  import Switch from '../Switch.svelte';
  import { nameThing } from '../settingsSections.svelte';
  import { OWN } from '../sources.svelte';
  import { toast } from '../toast.svelte';
  import { withUndo } from '../undo.svelte';
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
  /** The users not read: said in place with Try again, never an empty list (the owner, 2026-11-02). */
  let loadError = $state<string | null>(null);
  const load = () =>
    getUsers().then(
      (u) => ((users = u), (loadError = null)),
      (e) => (loadError = e instanceof Error ? e.message : "The users couldn't be read."),
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

  // Their own music, as the Library lists it.
  $effect(() => {
    if (open?.own_library && !library.view) readLibrary(false).catch(() => {});
  });
  const ownSource = $derived(library.view?.sources.find((s) => s.id === OWN) ?? null);
  const musicOf = (u: User) => (u.own_library ? (ownSource?.albums ?? []).filter((a) => librariesOf(a).includes(u.own_library!)) : []);
  const mains = $derived((ownSource?.library_info ?? []).filter((l) => l.kind === 'main'));
  let deleting = $state<ListedAlbum | null>(null);
  /** Delete all of a removed account's music: asked twice, as a main library's (decision 0101 point 7). */
  let deletingAll = $state<0 | 1 | 2>(0);
  let askedOnce = false;
  let moving = $state<LibraryInfo | null>(null);
  const reread = () => readLibrary(true);
  async function deleteOne(a: ListedAlbum, from: string) {
    try {
      const r = await deleteOwnAlbum(OWN, a.id, from);
      await reread();
      withUndo(`Deleted ${a.title}.`, [r.undo], { after: reread });
    } catch (e) {
      toast.fail(e, 'Could not delete it');
    }
  }
  /** Each of them, with one Undo. */
  async function forAll(them: ListedAlbum[], each: (a: ListedAlbum) => Promise<string | null>, said: (n: number) => string, failure: string) {
    const tokens: (string | null)[] = [];
    await fewAtATime(them, async (a) => void tokens.push(await each(a)), 1);
    await reread();
    if (tokens.length) withUndo(said(tokens.length), tokens, { after: reread });
    if (tokens.length < them.length) toast.fail(null, failure);
  }
  const moveAll = (u: User, to: LibraryInfo) =>
    forAll(musicOf(u), async (a) => (await moveAlbum(OWN, a.id, u.own_library!, to.id)).undo, (n) => `Moved ${plural(n, 'record')} to ${to.name}.`, 'Not all could be moved');
  const deleteAll = (u: User) => forAll(musicOf(u), async (a) => (await deleteOwnAlbum(OWN, a.id, u.own_library!)).undo, (n) => `Deleted ${plural(n, 'record')}.`, 'Not all could be deleted');

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

{#if loadError}<Notice text={loadError} onretry={load} />{/if}
{#if users === null}
  {#if !loadError}<PageLoading />{/if}
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
  {#if musicOf(u).length}
    <Group title={`${u.name}'s music`} more="Deleting here is the one way to delete from someone else's music. It goes to the trash, with Undo.">
      {#each musicOf(u) as a (a.id)}
        <Item name={a.title} line={a.artist}><button onclick={() => (deleting = a)}>Delete</button></Item>
      {/each}
    </Group>
    {#if !u.active}
      <Group>
        {#each mains as m (m.id)}<Item name={`Move its music into ${m.name}`} onclick={() => (moving = m)} />{/each}
        <Item name="Delete its music" kind="danger" onclick={() => (deletingAll = 1)} />
      </Group>
    {/if}
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
  <ConfirmSheet title={`Remove ${u.name}?`} text="Switched off; their music, playlists and history are kept." label="Remove" onconfirm={() => activate(u, false)} onclose={() => (removing = null)} />
{/if}
{#if deleting}
  {@const a = deleting}
  <ConfirmSheet title={`Delete ${a.title}?`} text={`From ${open?.name}'s music, into the trash.`} label="Delete" onconfirm={() => deleteOne(a, open!.own_library!)} onclose={() => (deleting = null)} />
{/if}
{#if moving && open}
  {@const u = open}
  {@const to = moving}
  <ConfirmSheet title={`Move ${plural(musicOf(u).length, 'record')} into ${to.name}?`} label="Move" icon="intoBox" danger={false} onconfirm={() => moveAll(u, to)} onclose={() => (moving = null)} />
{/if}
{#if deletingAll === 1 && open}
  {@const u = open}
  <ConfirmSheet title={`Delete all of ${u.name}'s music?`} text={`${plural(musicOf(u).length, 'record')} into the trash.`} label="Delete" onconfirm={() => (askedOnce = true)} onclose={() => ((deletingAll = askedOnce ? 2 : 0), (askedOnce = false))} />
{:else if deletingAll === 2 && open}
  {@const u = open}
  <ConfirmSheet title="Really delete it all?" text={`All ${plural(musicOf(u).length, 'record')} of ${u.name}'s music.`} label="Delete it all" onconfirm={() => deleteAll(u)} onclose={() => (deletingAll = 0)} />
{/if}

<style>
  .first {
    margin: 0;
    color: var(--muted);
    font-size: 13px;
  }
</style>
