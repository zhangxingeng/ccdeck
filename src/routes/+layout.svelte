<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import CopyContextMenu from '$lib/components/CopyContextMenu.svelte';
  import UpdateBanner from '$lib/components/UpdateBanner.svelte';
  import { checkForUpdates } from '$lib/updater.svelte';
  let { children } = $props();

  onMount(() => {
    // Only in the packaged desktop app — skip in browser preview/dev.
    if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
      checkForUpdates(true);
    }
  });
</script>

{@render children()}
<CopyContextMenu />
<UpdateBanner />
