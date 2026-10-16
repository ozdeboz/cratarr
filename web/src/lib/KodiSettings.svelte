<script lang="ts">
  // Settings → Players → Kodi (the owner, 2026-10-16): the Kodis Cratarr plays on, driven over Kodi's own remote
  // control ("Allow remote control via HTTP"), nothing installed on Kodi. Each becomes one of the players: "Play on"
  // reaches it, and a group takes it in as a follower. Its password is never shown again (blank keeps it).
  import { getPlayOnSettings, savePlayOnSettings, type KodiSetting } from './api';
  import Icon from './Icon.svelte';
  import { toast } from './toast.svelte';

  let webhooks = $state<string[]>([]);
  let kodis = $state<KodiSetting[]>([]);
  let base = $state<string | null>(null);
  let dirty = $state(false);
  let saving = $state(false);
  let loaded = $state(false);

  function take(s: { webhooks: string[]; kodi: KodiSetting[]; stream_default: string | null }) {
    webhooks = s.webhooks;
    kodis = s.kodi.map((k) => ({ ...k, password: '' }));
    base = s.stream_default;
    dirty = false;
    loaded = true;
  }

  $effect(() => {
    getPlayOnSettings().then(take, (e) => toast.fail(e, "The players' settings weren't read"));
  });

  function add() {
    kodis = [...kodis, { name: kodis.length ? `Kodi ${kodis.length + 1}` : 'Kodi', url: 'http://', user: 'kodi', password: '', stream_from: '' }];
    dirty = true;
  }

  function remove(i: number) {
    kodis = kodis.filter((_, j) => j !== i);
    dirty = true;
  }

  async function save() {
    saving = true;
    try {
      take(await savePlayOnSettings({ webhooks, kodi: kodis.map(({ name, url, user, password, stream_from }) => ({ name, url, user, password: password ?? '', stream_from })) }));
    } catch (e) {
      toast.fail(e, "Kodi wasn't saved");
    } finally {
      saving = false;
    }
  }
</script>

{#if loaded}
  <section class="kodi" aria-label="Kodi">
    <div class="head">
      <span>Kodi</span>
      <button class="icon" aria-label="Add a Kodi" title="Add a Kodi" onclick={add}><Icon name="plus" size={22} /></button>
    </div>
    {#each kodis as k, i (i)}
      <div class="one">
        <label><span>Name</span><input type="text" bind:value={k.name} oninput={() => (dirty = true)} /></label>
        <label><span>Address</span><input type="url" placeholder="http://kodi:8080" spellcheck="false" autocapitalize="off" bind:value={k.url} oninput={() => (dirty = true)} /></label>
        <label><span>User</span><input type="text" autocapitalize="off" spellcheck="false" bind:value={k.user} oninput={() => (dirty = true)} /></label>
        <label
          ><span>Password</span><input
            type="password"
            autocomplete="new-password"
            placeholder={k.password_set ? 'Saved' : ''}
            bind:value={k.password}
            oninput={() => (dirty = true)}
          /></label
        >
        <label
          ><span>Cratarr for Kodi</span><input
            type="url"
            placeholder={base ?? 'http://cratarr:8081'}
            spellcheck="false"
            autocapitalize="off"
            bind:value={k.stream_from}
            oninput={() => (dirty = true)}
          /></label
        >
        <button class="icon danger" aria-label="Remove {k.name}" title="Remove" onclick={() => remove(i)}><Icon name="trash" size={20} /></button>
      </div>
    {/each}
    {#if dirty}
      <div class="bar">
        <button class="primary" disabled={saving} aria-busy={saving} onclick={save}>Save</button>
        <button disabled={saving} onclick={() => getPlayOnSettings().then(take)}>Discard</button>
      </div>
    {/if}
  </section>
{/if}

<style>
  .kodi {
    display: grid;
    gap: 10px;
    padding: 4px 0 16px;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 16px;
    font-weight: 600;
  }

  .one {
    position: relative;
    display: grid;
    gap: 8px;
    padding: 12px 52px 12px 16px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--text) 5%, transparent);
  }

  label {
    display: grid;
    grid-template-columns: minmax(96px, 30%) 1fr;
    align-items: center;
    gap: 10px;
    font-size: 15px;
  }

  label span {
    color: var(--muted);
  }

  input {
    min-width: 0;
    font: inherit;
  }

  .one .icon {
    position: absolute;
    top: 8px;
    right: 8px;
  }

  .icon {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--art-accent, var(--accent));
    cursor: pointer;
  }

  .icon.danger {
    color: var(--danger);
  }

  .bar {
    display: flex;
    gap: 16px;
  }
</style>
