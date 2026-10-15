<script lang="ts">
  // Cratarr's mark as the app icon draws it (the record on its dark green, the corner rounded as the icon's), in the
  // two-ring version the owner chose for about 32 px: the sidebar's brand. The path is tools/brand/record.py's `two`.
  let { size = 34 }: { size?: number } = $props();
</script>

<svg width={size} height={size} viewBox="18 18 72 72" aria-hidden="true"
  ><rect x="18" y="18" width="72" height="72" rx="15.75" fill="#06281A" /><path
    fill="#3DDC97"
    d="M54.0000,54.0000L56.5981,55.5000A3,3 0,1,1 56.5981,52.5000ZM65.0851,60.4000A12.8,12.8 0,1,1 65.0851,47.6000L60.9282,50.0000A8,8 0,1,0 60.9282,58.0000ZM69.8483,63.1500A18.3,18.3 0,1,1 69.8483,44.8500L67.1636,46.4000A15.2,15.2 0,1,0 67.1636,61.6000ZM74.7846,66.0000A24,24 0,1,1 74.7846,42.0000L71.9267,43.6500A20.7,20.7 0,1,0 71.9267,64.3500ZM67.1636,46.4000A15.2,15.2 0,0,1 67.1636,61.6000L65.0851,60.4000A12.8,12.8 0,0,0 65.0851,47.6000ZM71.9267,43.6500A20.7,20.7 0,0,1 71.9267,64.3500L69.8483,63.1500A18.3,18.3 0,0,0 69.8483,44.8500Z"
  /></svg
>
