<script lang="ts">
  // A browser asking to be signed in as you (QR07; QR06 the server's): the sheet every question has, saying what asked (its device,
  // when, and whether from home: OQ-B1 (a)), Allow and Refuse; Allow asks the account's password again first (OQ-B2 (a)), in the
  // sheet a password is typed in everywhere. One request each, however often pressed, and nothing said after: Account's Signed in
  // lists the browser when next shown. A code that does not work: one line, whatever the cause. Out of reach: the line every page
  // says then, and no Allow. Shown by App, from the QR's link (`/pair/allow?code=…`) or a code typed in Account.
  import { untrack } from 'svelte';
  import { pairAllow, pairAsk, pairRefuse } from './api';
  import { loadMe } from './me.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { allowShows, askLine, DEAD_CODE, type PairAsk } from './pairing';
  import { reach } from './reach.svelte';
  import FormSheet from './settings/FormSheet.svelte';
  import { toast } from './toast.svelte';

  let { code, onclose }: { code: string; onclose: () => void } = $props();
  const asked = untrack(() => code);

  /** What asked, once read; 'dead': the code does not work. */
  let read = $state<PairAsk | 'dead' | null>(null);
  pairAsk(asked).then(
    (a) => (read = a),
    () => (read = 'dead'),
  );
  const shows = $derived(allowShows(read, reach.away));

  /** Allow pressed: the password asked; never kept after its request. */
  let typing = $state(false);
  let password = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);
  async function allow() {
    if (busy || !password) return;
    busy = true;
    error = null;
    try {
      await pairAllow(asked, password);
      void loadMe();
      onclose();
    } catch (e) {
      // The server's words ("Wrong password.", "Too many tries…"), as Settings shows them.
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
      password = '';
    }
  }
  async function refuse() {
    if (busy) return;
    busy = true;
    try {
      await pairRefuse(asked);
      onclose();
    } catch (e) {
      toast.fail(e, "It wasn't refused");
    } finally {
      busy = false;
    }
  }
</script>

{#if typing}
  <FormSheet title="Allow" submitLabel="Allow" canSubmit={!!password} {busy} {error} onsubmit={() => void allow()} onclose={() => ((typing = false), (password = ''), (error = null))}>
    <!-- svelte-ignore a11y_autofocus -->
    <label>Your password<input type="password" bind:value={password} autocomplete="current-password" autofocus /></label>
  </FormSheet>
{:else}
  <MenuSheet label="Sign in a browser" icon="key" title="This signs in a browser as you" {onclose}>
    {#if shows === 'away'}
      <p class="text">Allowing a device needs Cratarr.</p>
    {:else if shows === 'dead'}
      <p class="text">{DEAD_CODE}</p>
    {:else if shows === 'ask' && read && read !== 'dead'}
      <p class="text">{askLine(read, Date.now() / 1000)}</p>
      <MenuEntry icon="check" label="Allow" onclick={() => (typing = true)} />
      <MenuEntry icon="ban" label="Refuse" danger onclick={() => void refuse()} />
    {/if}
  </MenuSheet>
{/if}

<style>
  /* As a ConfirmSheet's words over its acts. */
  .text {
    margin: 0;
    padding: 4px 24px 12px;
    color: var(--muted);
  }
</style>
