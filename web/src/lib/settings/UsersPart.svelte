<script lang="ts">
  // Users (decision 0051; the owner: "a user panel with a permissions section ... It's probably how Jellyfin does it
  // too"): every account, its name and role from the directory (read-only: they are set there), whether it is active
  // (one who left the directory's music group is deactivated, their data kept), what it may do beyond listening, and its
  // devices and sign-ins, each to end. Its administrators' only. Acts at once.
  import { endSession, getUsers, removable, removeDevice, setPermissions, type OwnDevice, type Permissions, type User, type UserSession } from '../api';
  import MoreInfo from '../MoreInfo.svelte';
  import PageLoading from '../PageLoading.svelte';
  import Switch from '../Switch.svelte';
  import { toast } from '../toast.svelte';
  import { when } from './when';

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

<section aria-labelledby="users-h">
  <h2 id="users-h">Users</h2>
  {#if users === null}
    <PageLoading />
  {:else}
    {#each users as u (u.name)}
      <details class="user">
        <summary>
          <span class="name">{u.name}</span>
          <span class="role">{u.admin ? 'Administrator' : 'User'}{#if !u.active} · Inactive{/if}</span>
        </summary>
        {#if !u.admin}
          <div class="group">
            <h3>Permissions <MoreInfo label="Permissions" text="Apps: OpenSubsonic apps. Administrators may do everything." /></h3>
          </div>
          <div class="checks">
            {#each PERMISSIONS as p (p.key)}
              <Switch label={p.label} checked={u.permissions[p.key]} disabled={!u.active} onchange={(on) => permit(u, p.key, on)} />
            {/each}
          </div>
        {/if}
        {#if u.devices.length}
          <div class="group"><h3>Devices</h3></div>
          <ul class="list">
            {#each u.devices as d (d.id)}
              <li>
                <span class="what">{d.name}<small>{d.kind}</small></span>
                {#if d.online}<span class="when">Online</span>{:else if d.last_seen}<span class="when">{when(d.last_seen)}</span>{/if}
                {#if removable(d)}<button onclick={() => remove(u, d)}>Remove</button>{/if}
              </li>
            {/each}
          </ul>
        {/if}
        {#if u.sessions.length}
          <div class="group"><h3>Signed in</h3></div>
          <ul class="list">
            {#each u.sessions as s (s.id)}
              <li>
                <span class="what">{s.device}</span>
                <span class="when">{when(s.last_used)}</span>
                <button onclick={() => end(u, s)}>Sign out</button>
              </li>
            {/each}
          </ul>
        {/if}
      </details>
    {/each}
  {/if}
</section>

<style>
  .user {
    border-top: 1px solid var(--border);
  }

  .user:first-of-type {
    border-top: 0;
  }

  .user summary {
    display: flex;
    align-items: baseline;
    gap: 12px;
    min-height: 52px;
    padding: 14px 0;
    cursor: pointer;
  }

  .user[open] summary {
    padding-bottom: 4px;
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
    font-weight: 600;
  }

  .role {
    color: var(--muted);
    white-space: nowrap;
  }

  .user[open] {
    padding-bottom: 12px;
  }

  .group {
    padding: 16px 0 4px;
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .list li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 0;
    border-top: 1px solid var(--border);
  }

  .what {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .what small {
    margin-left: 8px;
    color: var(--muted);
  }

  .when {
    color: var(--muted);
    white-space: nowrap;
  }
</style>
