<script lang="ts">
  // The dice held (decision 0057 §4d): what to randomize. Everything (as a tap does), a category alone, or the cables'
  // amounts; pinned knobs stay as they are.
  import MenuEntry from '../MenuEntry.svelte';
  import MenuSheet from '../MenuSheet.svelte';
  import { RANDOMIZE_SCOPES } from './ports';

  type Scope = (typeof RANDOMIZE_SCOPES)[number];
  let { onroll, onclose }: { onroll: (scope: Scope) => void; onclose: () => void } = $props();
  const LABEL: Record<Scope, string> = { everything: 'Everything', sources: 'Sources', effects: 'Effects', mixers: 'Mixers', modulators: 'Modulators', cables: 'Cables' };
</script>

<MenuSheet label="Randomize" title="Randomize" icon="dice" over {onclose}>
  {#each RANDOMIZE_SCOPES as scope (scope)}
    <MenuEntry label={LABEL[scope]} onclick={() => (onroll(scope), onclose())} />
  {/each}
</MenuSheet>
