<script lang="ts">
  // Appearance (the owner, 2026-10-25: Display's concerns apart): this browser's look, acting at once: Light or Dark as
  // two pictures with Automatic under them (lib/settings/AppearancePicker), and Animations; Home's header (the owner,
  // 2026-10-25): a personal greeting in place of the moment's name, and a picture of one's own in place of the
  // weather's sky, kept on this device (lib/localCovers).
  import { slide } from 'svelte/transition';
  import { inApp } from '../app.svelte';
  import { prefs } from '../prefs.svelte';
  import AppearancePicker from './AppearancePicker.svelte';
  import BackdropPicker from './BackdropPicker.svelte';
  import { BACKDROPS, DEFAULT_BACKDROP, nextBackdrop, shuffleNow } from '../backdrop.svelte';
  import Group from './Group.svelte';
  import ChoiceSheet from './ChoiceSheet.svelte';
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
      void removeLocalPicture(HOME_HEADER);
      prefs.flowBackdrop = DEFAULT_BACKDROP;
      prefs.flowPictureOnHome = false;
      prefs.flowShuffle = false;
      prefs.coverColourGlass = prefs.coverColourMini = prefs.coverColourPlayer = true;
    },
  );

  const SHUFFLE_WHEN: { value: typeof prefs.flowShuffleWhen; label: string }[] = [
    { value: 'flow', label: 'Each flow that opens' },
    { value: 'seeded', label: 'Each flow its own' },
    { value: 'start', label: 'Each time Cratarr opens' },
    { value: 'timed', label: 'Every five minutes' },
  ];
  let whenOpen = $state(false);

  let busy = $state(false);
  /** One's own picture gone: the default photo again where it stood behind the flows. */
  function dropOwn() {
    void removeLocalPicture(HOME_HEADER);
    if (prefs.flowBackdrop === 'own') prefs.flowBackdrop = DEFAULT_BACKDROP;
  }
  async function pick(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file) return;
    busy = true;
    try {
      await setLocalPicture(HOME_HEADER, file, HOME_HEADER_PX);
      // Chosen, it is what stands behind the flows.
      prefs.flowBackdrop = 'own';
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
</Group>

<!-- The playing cover's colour, each place apart (the owner, 2026-10-27): off, Cratarr's own there. -->
<Group title="Cover colours">
  <Item name="Control glass" toggle={{ checked: prefs.coverColourGlass, onchange: (on) => (prefs.coverColourGlass = on) }} />
  <Item name="Mini player" toggle={{ checked: prefs.coverColourMini, onchange: (on) => (prefs.coverColourMini = on) }} />
  <Item name="Full player background" toggle={{ checked: prefs.coverColourPlayer, onchange: (on) => (prefs.coverColourPlayer = on) }} />
</Group>

<!-- Behind every flow (crates', a wide album header's, the Library's Flow, Discover's; the owner, 2026-10-26): Cratarr's
     photos or one's own; Home's sky too only when asked. -->
<Group title="Behind the flows">
  <BackdropPicker />
  <!-- Shuffle (the owner, 2026-10-27): the picker's ticks turn to boxes; when another comes is the user's to say. -->
  <Item name="Shuffle" toggle={{ checked: prefs.flowShuffle, onchange: (on) => (prefs.flowShuffle = on) }} />
  {#if prefs.flowShuffle}
    <div transition:slide={{ duration: 160 }}>
      <Item name="Another one" value={SHUFFLE_WHEN.find((o) => o.value === prefs.flowShuffleWhen)?.label ?? null} onclick={() => (whenOpen = true)} />
      <!-- While one stands everywhere (till the next start, or the timer), the user can take the next one now (the owner,
           2026-10-27). -->
      {#if prefs.flowShuffleWhen === 'start' || prefs.flowShuffleWhen === 'timed'}
        <Item name="Now" line={BACKDROPS.find((b) => b.value === shuffleNow())?.label ?? (shuffleNow() === 'own' ? 'Yours' : null)}>
          <span class="pick"><button onclick={nextBackdrop}>Next</button></span>
        </Item>
      {/if}
    </div>
  {/if}
  <Item name="Your picture" line={localCovers[HOME_HEADER] ? 'On this device' : 'One of your own, kept on this device'}>
    <span class="pick">
      <label class="button" class:busy>{localCovers[HOME_HEADER] ? 'Change' : 'Choose a picture'}<input type="file" accept="image/*" hidden disabled={busy} onchange={pick} /></label>
      {#if localCovers[HOME_HEADER]}<button onclick={dropOwn}>Remove</button>{/if}
    </span>
  </Item>
  <Item name="Also on Home" line="In place of the weather's sky" toggle={{ checked: prefs.flowPictureOnHome, onchange: (on) => (prefs.flowPictureOnHome = on) }} />
</Group>

{#if whenOpen}
  <ChoiceSheet title="Another one" options={SHUFFLE_WHEN} value={prefs.flowShuffleWhen} onchange={(v) => (prefs.flowShuffleWhen = v)} onclose={() => (whenOpen = false)} />
{/if}

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
