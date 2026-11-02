<script lang="ts">
  // A connection's API keys (decisions 0043, 0051; the owner, 2026-10-24: kept with what uses them, under Connections):
  // each key with whom it acts for, what it may do, when made and last used, to revoke; a new one asks for the password
  // again and shows once, here, with Copy. Home Assistant's (lib/settings/HomeAssistantPart: whom it acts for, Control
  // or Read only) and MCP's (lib/settings/McpPart: only ever reading). Acts at once, not with the Save bar.
  import { newApiKey, revokeApiKey, type ApiKey, type KeyPurpose, type KeyScope } from '../api';
  import { copyText } from '../clipboard';
  import { me } from '../me.svelte';
  import Seg from '../Seg.svelte';
  import FormSheet from './FormSheet.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import { when } from './when';

  let {
    keys,
    name,
    purpose,
    scopes = null,
    owners = [],
    onchange,
  }: {
    keys: readonly ApiKey[];
    /** The name a new key gets. */
    name: string;
    purpose: KeyPurpose;
    /** What a new key may do, chosen (Control first); none: only reading. */
    scopes?: { value: KeyScope; label: string }[] | null;
    /** Whom a new key may act for, chosen where there are several (an administrator's users). */
    owners?: string[];
    /** A key made or revoked: read them again. */
    onchange: () => void;
  } = $props();

  const MORE = 'A key shows once, when it is made; revoking one ends it at once.';

  let error = $state<string | null>(null);
  const failed = (e: unknown) => (error = e instanceof Error ? e.message : String(e));

  // A new key: whom it acts for (oneself unless chosen), what it may do, and the password; shown here once.
  let making = $state(false);
  let busy = $state(false);
  let keyOwner = $state('');
  let keyScope = $state<KeyScope>('control');
  let keyPassword = $state('');
  let made = $state<{ id: string; key: string } | null>(null);
  async function makeKey() {
    error = null;
    busy = true;
    try {
      const k = await newApiKey(name, keyPassword, keyOwner || undefined, scopes ? keyScope : 'read', purpose);
      made = { id: k.id, key: k.key };
      making = false;
      onchange();
    } catch (e) {
      failed(e);
    } finally {
      keyPassword = '';
      busy = false;
    }
  }

  async function revoke(k: ApiKey) {
    error = null;
    try {
      await revokeApiKey(k.id);
      if (made?.id === k.id) made = null;
      onchange();
    } catch (e) {
      failed(e);
    }
  }

  const keyLine = (k: ApiKey) =>
    [k.owner, scopes && k.scope === 'read' ? 'Read only' : null, `Made ${when(k.created)}`, k.last_used ? `used ${when(k.last_used)}` : 'not used yet']
      .filter(Boolean)
      .join(' · ');
</script>

<Group title="API key" more={MORE}>
  {#each keys as k (k.id)}
    {#if made?.id === k.id}
      {@const m = made}
      <Item lead="on" name="Shown only now" line={m.key} wrap>
        <button onclick={() => copyText(m.key)}>Copy</button>
      </Item>
    {:else}
      <Item name={k.name} line={keyLine(k)}>
        <button onclick={() => revoke(k)}>Revoke</button>
      </Item>
    {/if}
  {/each}
  <Item name="New key" kind="add" onclick={() => ((error = null), (making = true))} />
</Group>
{#if error && !making}<Group><Item lead="bad" name={error} /></Group>{/if}
{#if making}
  <FormSheet title="New key" submitLabel="Make key" canSubmit={!!keyPassword} {busy} {error} onsubmit={makeKey} onclose={() => ((making = false), (keyPassword = ''), (error = null))}>
    {#if owners.length > 1}
      <label
        >Acts for<select bind:value={keyOwner}>
          {#each owners as o (o)}<option value={o === me.name ? '' : o}>{o}</option>{/each}
        </select></label
      >
    {/if}
    {#if scopes}<span class="scope"><Seg label="Scope" options={scopes} bind:value={keyScope} /></span>{/if}
    <!-- svelte-ignore a11y_autofocus -->
    <label>Password<input type="password" bind:value={keyPassword} autocomplete="current-password" autofocus /></label>
  </FormSheet>
{/if}

<style>
  .scope {
    display: flex;
  }
</style>
