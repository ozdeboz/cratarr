<script lang="ts">
  // Connections' MCP (0.13, decision 0090): an assistant reads Cratarr over MCP with a key of its own, kept here as
  // Home Assistant's are (the owner, 2026-10-24). In the list (`page` off), a row with when a key was last used; on its
  // page, the address to give the assistant, to copy, and its keys (lib/settings/KeyGroup), which only ever read.
  import { getApiKeys, type ApiKey } from '../api';
  import { keysFor, lastUsed, mcpAddress } from '../apiKeys';
  import { copyText } from '../clipboard';
  import { openThing } from '../panel.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import KeyGroup from './KeyGroup.svelte';
  import { when } from './when';

  let { page = false }: { page?: boolean } = $props();

  const MORE = 'An assistant that speaks MCP reads your library, plays and lists through this address, with a key made here.';
  const address = mcpAddress(location.origin);

  let keys = $state<ApiKey[] | null>(null);
  const load = () =>
    getApiKeys().then(
      (r) => (keys = keysFor(r.keys, 'mcp')),
      () => (keys = []),
    );
  load();

  const line = $derived.by(() => {
    if (keys === null) return null;
    if (!keys.length) return 'No key yet';
    const at = lastUsed(keys);
    return at ? `Last used ${when(at)}` : 'Not used yet';
  });
</script>

{#if !page}
  <Group title="Assistants" more={MORE}>
    {#if line}<Item lead={keys?.length ? 'on' : 'off'} name="MCP" {line} onclick={() => openThing('connections', 'mcp')} />{/if}
  </Group>
{:else}
  <Group title="Address" more={MORE}>
    <Item name={address} line="Streamable HTTP, the key as a Bearer token" wrap>
      <button onclick={() => copyText(address)}>Copy</button>
    </Item>
  </Group>
  <KeyGroup keys={keys ?? []} name="MCP" purpose="mcp" onchange={load} />
{/if}
