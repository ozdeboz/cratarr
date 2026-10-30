<script lang="ts">
  // Cratarr's mark, the record itself (round), in its three-ring version: the sidebar's brand (decided 2026-10-15; no
  // longer in back's place on a phone's page row, 2026-10-16). `size` is its diameter. The path is tools/brand/record.py's
  // `full`; the view is the record's own square (radius 24 about 54, 54). lib/Cover's stand-in draws it in `fill`
  // "currentColor".
  let { size = 34, fill = '#12A36A' }: { size?: number; fill?: string } = $props();
</script>

<svg width={size} height={size} viewBox="30 30 48 48" aria-hidden="true"><path {fill} d="M54.0000,54.0000L56.5981,55.5000A3,3 0,1,1 56.5981,52.5000ZM64.7387,60.2000A12.4,12.4 0,1,1 64.7387,47.8000L60.9282,50.0000A8,8 0,1,0 60.9282,58.0000ZM68.2028,62.2000A16.4,16.4 0,1,1 68.2028,45.8000L65.7779,47.2000A13.6,13.6 0,1,0 65.7779,60.8000ZM71.6669,64.2000A20.4,20.4 0,1,1 71.6669,43.8000L69.2420,45.2000A17.6,17.6 0,1,0 69.2420,62.8000ZM74.7846,66.0000A24,24 0,1,1 74.7846,42.0000L72.7061,43.2000A21.6,21.6 0,1,0 72.7061,64.8000ZM65.7779,47.2000A13.6,13.6 0,0,1 65.7779,60.8000L64.7387,60.2000A12.4,12.4 0,0,0 64.7387,47.8000ZM69.2420,45.2000A17.6,17.6 0,0,1 69.2420,62.8000L68.2028,62.2000A16.4,16.4 0,0,0 68.2028,45.8000ZM72.7061,43.2000A21.6,21.6 0,0,1 72.7061,64.8000L71.6669,64.2000A20.4,20.4 0,0,0 71.6669,43.8000Z" /></svg>
