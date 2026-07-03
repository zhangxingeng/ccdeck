<script lang="ts">
  import '../app.css';
  import CopyContextMenu from '$lib/components/CopyContextMenu.svelte';
  let { children } = $props();
</script>

{@render children()}
<CopyContextMenu />
