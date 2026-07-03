/**
 * Clipboard helper shared by copy-to-clipboard affordances (context menu,
 * copy buttons). Pure DOM — no Tauri dependency, since the async Clipboard
 * API works fine inside the Tauri webview.
 */

/**
 * Copy `text` to the OS clipboard. Prefers the async Clipboard API; falls
 * back to a hidden textarea + execCommand for contexts where it's unavailable.
 * Returns whether the copy is believed to have succeeded.
 */
export async function copyToClipboard(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard && window.isSecureContext) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    // fall through to legacy fallback
  }
  try {
    const ta = document.createElement('textarea');
    ta.value = text;
    ta.style.position = 'fixed';
    ta.style.opacity = '0';
    document.body.appendChild(ta);
    ta.focus();
    ta.select();
    const ok = document.execCommand('copy');
    document.body.removeChild(ta);
    return ok;
  } catch {
    return false;
  }
}
