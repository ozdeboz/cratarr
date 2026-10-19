<script lang="ts">
  // Account (decision 0051, several users): who is signed in and their role, both from the directory and read-only;
  // the address apps reach Cratarr at; their own password for OpenSubsonic apps (whose sign-in needs the password
  // itself, which a directory never gives); their devices (each plays only on its own) and their sign-ins, each to end.
  // Acts at once.
  import { endMySession, removable, removeMyDevice, setAppPassword, type OwnDevice, type UserSession } from '../api';
  import { loadMe, me } from '../me.svelte';
  import { toast } from '../toast.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import ValueSheet from './ValueSheet.svelte';
  import { when } from './when';

  let setting = $state(false);
  async function savePassword(password: string) {
    try {
      await setAppPassword(password);
      await loadMe();
    } catch (e) {
      toast.fail(e, 'Could not set the password');
      throw e;
    }
  }

  async function end(s: UserSession) {
    try {
      await endMySession(s.id);
      await loadMe();
    } catch (e) {
      toast.fail(e, 'Could not sign it out');
    }
  }

  async function remove(d: OwnDevice) {
    try {
      await removeMyDevice(d.id);
      await loadMe();
    } catch (e) {
      toast.fail(e, 'Could not remove it');
    }
  }
</script>

<Group>
  {#if me.name}<Item name={me.name} line={me.admin ? 'Administrator' : 'User'} />{:else}<Item name={me.admin ? 'Administrator' : 'User'} />{/if}
  <Item name="Address" line={location.origin} wrap />
  {#if me.name}
    <Item
      name="App password"
      more="OpenSubsonic apps sign in with your name and this password, never your directory password."
      value={me.app_password ? 'Set' : 'Not set'}
      onclick={() => (setting = true)}
    />
  {/if}
</Group>

{#if me.devices?.length}
  <Group title="Devices">
    {#each me.devices as d (d.id)}
      <Item lead={d.online ? 'on' : 'off'} name={d.name} line={`${d.kind} · ${d.online ? 'online' : d.last_seen ? when(d.last_seen) : 'offline'}`}>
        {#if removable(d)}<button onclick={() => remove(d)}>Remove</button>{/if}
      </Item>
    {/each}
  </Group>
{/if}

{#if me.sessions?.length}
  <Group title="Signed in">
    {#each me.sessions as s (s.id)}
      <Item name={s.device} line={s.current ? 'This device' : when(s.last_used)}>
        {#if s.current}
          <!-- Signing out here clears this browser's cookie too. -->
          <form method="post" action="/signout"><button>Sign out</button></form>
        {:else}
          <button onclick={() => end(s)}>Sign out</button>
        {/if}
      </Item>
    {/each}
  </Group>
{/if}

{#if setting}
  <ValueSheet title="App password" type="password" value="" placeholder="New password" allowEmpty={false} saveLabel="Save" onsave={savePassword} onclose={() => (setting = false)} />
{/if}
