<script lang="ts">
  // A Knob (face.ts's blocks): the graph's knob (PatchKnob: a dial with its value's arc, its cables' ring, its pin mark;
  // turned by a drag, the wheel or the arrows; held, fine-adjusted), for the graph and the panel alike.
  import type { ComponentProps } from 'svelte';
  import PatchKnob from '../PatchKnob.svelte';

  let props: ComponentProps<typeof PatchKnob> = $props();
</script>

<PatchKnob {...props} />
