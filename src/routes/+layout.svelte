<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import CopyContextMenu from '$lib/components/CopyContextMenu.svelte';
  let { children } = $props();

  onMount(() => {
    // Only in the packaged desktop app — skip in browser preview/dev.
    if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
      import('$lib/updater').then((m) => m.checkForUpdates(true));
    }
  });
</script>

{@render children()}
<CopyContextMenu />
