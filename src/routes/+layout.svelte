<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import CopyContextMenu from '$lib/components/CopyContextMenu.svelte';
  import UpdateBanner from '$lib/components/UpdateBanner.svelte';
  import { checkForUpdates } from '$lib/updater.svelte';
  import { isTauri } from '$lib/api';
  let { children } = $props();

  onMount(() => {
    // Only in the packaged desktop app — skip in browser preview/dev.
    if (!isTauri()) return;
    checkForUpdates(true);

    // Rendered message content ({@html}'d markdown) contains plain <a href>
    // tags with no click handling. Left alone, clicking one makes the Tauri
    // webview itself navigate (no CSP blocks it), replacing the whole SPA
    // with no back button. Intercept every link click app-wide here and hand
    // it to the OS default app/browser instead.
    function onClick(e: MouseEvent) {
      const anchor = (e.target as HTMLElement).closest('a[href]') as HTMLAnchorElement | null;
      if (!anchor) return;
      const href = anchor.getAttribute('href');
      if (!href) return;
      e.preventDefault();
      const isSchemed = /^[a-z][a-z0-9+.-]*:/i.test(href);
      import('@tauri-apps/plugin-opener').then(({ openUrl, openPath }) =>
        isSchemed ? openUrl(href) : openPath(href)
      );
    }
    document.addEventListener('click', onClick);
    return () => document.removeEventListener('click', onClick);
  });
</script>

{@render children()}
<CopyContextMenu />
<UpdateBanner />
