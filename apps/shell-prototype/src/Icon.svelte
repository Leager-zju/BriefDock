<script lang="ts">
  let { name, size = 18 }: { name: string; size?: number } = $props();
  const paths: Record<string, string> = {
    box: 'M3 7h18v14H3z M3 7l3-4h12l3 4 M9 11h6',
    settings: 'M4 7h16 M4 17h16 M8 4v6 M16 14v6',
    minus: 'M5 12h14', close: 'M6 6l12 12 M18 6L6 18',
    chevron: 'M8 10l4 4 4-4', refresh: 'M20 7v5h-5 M4 17v-5h5 M6 7a7 7 0 0 1 12-1l2 6 M4 12l2 6a7 7 0 0 0 12-1',
    play: 'M8 5l11 7-11 7z', pause: 'M8 5v14 M16 5v14',
    next: 'M5 5l10 7-10 7z M19 5v14', previous: 'M19 5L9 12l10 7z M5 5v14',
    volume: 'M4 9h4l5-5v16l-5-5H4z M17 8a6 6 0 0 1 0 8',
    mute: 'M4 9h4l5-5v16l-5-5H4z M17 9l5 6 M22 9l-5 6',
    moon: 'M20 14A8 8 0 0 1 10 4a8 8 0 1 0 10 10z',
    sun: 'M12 3v2 M12 19v2 M3 12h2 M19 12h2 M5.5 5.5L7 7 M17 17l1.5 1.5 M5.5 18.5L7 17 M17 7l1.5-1.5 M16 12a4 4 0 1 1-8 0 4 4 0 0 1 8 0',
    arrow: 'M19 12H5 M11 6l-6 6 6 6', clock: 'M12 7v5l3 2 M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0',
    check: 'M5 12l4 4L19 6', search: 'M21 21l-5-5 M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0',
    star: 'M12 3l3 6 6 1-4.5 4.5 1 6.5-5.5-3-5.5 3 1-6.5L3 10l6-1z'
  };
</script>
<svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d={paths[name] ?? paths.box}/></svg>
