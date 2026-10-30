<script lang="ts" generics="T extends string">
  // One of several, chosen in a sheet (the settings' North Star, the owner 2026-10-19): a row says the choice, a tap
  // opens the choices as round marks (lib/ChoiceList); a choice is taken at once and the sheet closes.
  import ChoiceList from '../ChoiceList.svelte';
  import MenuSheet from '../MenuSheet.svelte';

  let {
    title,
    options,
    value,
    onchange,
    onclose,
  }: {
    title: string;
    options: { value: T; label: string }[];
    value: T;
    onchange: (v: T) => void;
    onclose: () => void;
  } = $props();
</script>

<MenuSheet label={title} {title} {onclose}>
  <div class="choices">
    <ChoiceList
      label={title}
      {options}
      {value}
      onchange={(v) => {
        onchange(v);
        onclose();
      }}
    />
  </div>
</MenuSheet>

<style>
  .choices {
    padding: 0 8px 8px;
    overflow-y: auto;
  }
</style>
