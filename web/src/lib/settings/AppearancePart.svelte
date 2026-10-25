<script lang="ts">
  // Appearance (the owner, 2026-10-25: Display's concerns apart): this browser's look, acting at once: Light or Dark as
  // two pictures with Automatic under them (lib/settings/AppearancePicker), and Animations.
  import { inApp } from '../app.svelte';
  import { prefs } from '../prefs.svelte';
  import AppearancePicker from './AppearancePicker.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import { offerReset } from './reset.svelte';

  offerReset(
    () => 'The theme following the system again, and the animations on.',
    () => {
      prefs.theme = 'auto';
      prefs.animations = true;
    },
  );
</script>

<!-- Where it is kept is the group's name: this browser (in the app, this device). -->
<Group title={inApp ? 'This device' : 'This browser'}>
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
