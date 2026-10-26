<script lang="ts">
  // Appearance (the owner, 2026-10-25: Display's concerns apart): this browser's look, acting at once: Light or Dark as
  // two pictures with Automatic under them (lib/settings/AppearancePicker), and Animations; Home's header (the owner,
  // 2026-10-25): a personal greeting in place of the moment's name, and a picture of one's own in place of the
  // weather's sky, kept on this device (lib/localCovers).
  import { slide } from 'svelte/transition';
  import { inApp } from '../app.svelte';
  import { prefs } from '../prefs.svelte';
  import AppearancePicker from './AppearancePicker.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import { offerReset } from './reset.svelte';
  import { HOME_HEADER, HOME_HEADER_PX, localCovers, removeLocalPicture, setLocalPicture } from '../localCovers.svelte';
  import { toast } from '../toast.svelte';

  offerReset(
    () => "The theme following the system again, the animations on; Home's header greeting you, its sky the weather's.",
    () => {
      prefs.theme = 'auto';
      prefs.animations = true;
      prefs.homeGreeting = true;
      prefs.homeGreetingCapital = false;
      void removeLocalPicture(HOME_HEADER);
    },
  );

  let busy = $state(false);
  async function pick(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file) return;
    busy = true;
    try {
      await setLocalPicture(HOME_HEADER, file, HOME_HEADER_PX);
    } catch (err) {
      toast.fail(err, "The picture couldn't be kept");
    } finally {
      busy = false;
    }
  }
</script>

<!-- Where it is kept is the group's name: this browser (in the app, this device). -->
<Group title="Theme">
  <!-- Light or dark as two pictures, Automatic under them (the owner, 2026-10-25: Apple's way, mockup T3). Automatic
       switched off keeps the look the system gave. -->
  <AppearancePicker />
  <Item
    name="Automatic"
    line="Follows the system"
    toggle={{
      checked: prefs.theme === 'auto',
      onchange: (on) => (prefs.theme = on ? 'auto' : matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'),
    }}
  />
  <Item name="Animations" toggle={{ checked: prefs.animations, onchange: (on) => (prefs.animations = on) }} />
</Group>

<Group title="Home's header">
  <Item
    name="Personal greeting"
    toggle={{ checked: prefs.homeGreeting, onchange: (on) => (prefs.homeGreeting = on) }}
  />
  <!-- Out from under the greeting while it is on (the owner, 2026-10-25): for an account named in lower case. -->
  {#if prefs.homeGreeting}
    <div transition:slide={{ duration: 160 }}>
      <Item name="Capital first letter" line="Of your name, in the greeting" toggle={{ checked: prefs.homeGreetingCapital, onchange: (on) => (prefs.homeGreetingCapital = on) }} />
    </div>
  {/if}
  <Item name="Your picture" line={localCovers[HOME_HEADER] ? 'Behind every flow, on this device' : 'None: the sky for the weather'}>
    <span class="pick">
      <label class="button" class:busy>{localCovers[HOME_HEADER] ? 'Change' : 'Choose a picture'}<input type="file" accept="image/*" hidden disabled={busy} onchange={pick} /></label>
      {#if localCovers[HOME_HEADER]}<button onclick={() => removeLocalPicture(HOME_HEADER)}>Remove</button>{/if}
    </span>
  </Item>
  <!-- It is every flow's backdrop (crates', a wide album header's, the Library's Flow, Discover's); Home's sky too only
       when asked (the owner, 2026-10-26). -->
  {#if localCovers[HOME_HEADER]}
    <Item name="Also on Home" line="In place of the weather's sky" toggle={{ checked: prefs.flowPictureOnHome, onchange: (on) => (prefs.flowPictureOnHome = on) }} />
  {/if}
</Group>

<style>
  .pick {
    display: flex;
    gap: 8px;
  }

  /* The file input's label as the row's button, as the settings' other buttons look. */
  .button,
  .pick button {
    padding: 6px 14px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: none;
    color: var(--text);
    font: inherit;
    font-size: 14px;
    cursor: pointer;
  }

  .button.busy {
    opacity: 0.5;
  }
</style>
