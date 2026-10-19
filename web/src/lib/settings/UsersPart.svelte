<script lang="ts">
  // Users (decision 0051; the owner: "a user panel with a permissions section ... It's probably how Jellyfin does it
  // too"): every account a row, its role and whether it is active in a line; its own page behind it (`thing`: the
  // name, the settings' North Star, 2026-10-19): its name and role from the directory (read-only: they are set there),
  // whether it is active (one who left the directory's music group is deactivated, their data kept), what it may do
  // beyond listening, and its devices and sign-ins, each to end. Its administrators' only. Acts at once.
  import { endSession, getUsers, removable, removeDevice, setPermissions, type OwnDevice, type Permissions, type User, type UserSession } from '../api';
  import { backFromThing, openThing } from '../panel.svelte';
  import PageLoading from '../PageLoading.svelte';
  import { nameThing } from '../settingsSections.svelte';
  import { toast } from '../toast.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import { when } from './when';

  let { thing = null }: { thing?: string | null } = $props();

  // Downloading and changing playlists bind OpenSubsonic apps; sharing playlists binds Cratarr too. Public share links
  // (`links`) wait until links are offered.
  const PERMISSIONS: { key: keyof Permissions; label: string }[] = [
    { key: 'offline', label: 'Download in apps' },
    { key: 'playlists', label: 'Change playlists in apps' },
    { key: 'share', label: 'Share playlists' },
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

  /** A user's line: role, and inactive or how many devices. */
  const lineOf = (u: User) =>
    [u.admin ? 'Administrator' : 'User', !u.active ? 'inactive' : u.devices.length ? `${u.devices.length} ${u.devices.length === 1 ? 'device' : 'devices'}` : null]
      .filter(Boolean)
      .join(' · ');

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
</script>

{#if users === null}
  <PageLoading />
{:else if open}
  {@const u = open}
  <Group>
    <Item name={u.admin ? 'Administrator' : 'User'} line={u.active ? 'From the directory' : 'Inactive: no longer in the music group'} tone={u.active ? null : 'warn'} />
  </Group>
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
{:else if !thing}
  <Group>
    {#each users as u (u.name)}
      <Item name={u.name} line={lineOf(u)} onclick={() => openThing('users', u.name)} />
    {:else}
      <Item name="No users yet" />
    {/each}
  </Group>
{/if}
