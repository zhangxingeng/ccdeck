<script lang="ts">
  /**
   * ResumeMenu.svelte — the copyable-facts popover that replaced the terminal
   * launcher (issue #34). The terminal launch was platform-specific and
   * unreliable; the replacement is deliberately the most flexible thing
   * possible — show the user the facts and let them act in their own terminal.
   *
   * Opened at the click point (fixed, clamped on-screen) from a Resume button
   * or the per-message fork affordance, and dismissed on any outside click,
   * scroll, blur, Escape, or a fresh right-click — mirroring the app's existing
   * CopyContextMenu / ProviderResumeMenu positioning pattern.
   *
   * It presents three facts as rows, each a one-click copy:
   *   - the ready-to-paste resume command (`cd '<cwd>' && claude --resume '<id>'`)
   *   - the project path (the session's real cwd)
   *   - the session id
   * The menu stays open after a copy so several facts can be grabbed in a row;
   * a copied row shows an inline "Copied ✓" tick and calls `onCopied` so the
   * host surfaces its own toast. Each value also carries `data-copy-text`, so a
   * right-click on it works through the global CopyContextMenu too.
   */
  import { copyToClipboard } from '$lib/copy';
  import { resumeCommand } from '$lib/resume';

  let {
    x,
    y,
    cwd,
    sessionId,
    heading = 'Resume in your terminal',
    onCopied,
    onClose,
  }: {
    x: number;
    y: number;
    cwd: string;
    sessionId: string;
    heading?: string;
    onCopied?: (label: string) => void;
    onClose: () => void;
  } = $props();

  interface Fact {
    label: string;
    value: string;
  }

  // Project path is omitted when the session recorded no cwd — there's nothing
  // to copy, and the resume command already drops the `cd` in that case.
  let facts = $derived<Fact[]>(
    [
      { label: 'Resume command', value: resumeCommand(cwd, sessionId) },
      cwd.trim() !== '' ? { label: 'Project path', value: cwd } : null,
      { label: 'Session id', value: sessionId },
    ].filter((f): f is Fact => f !== null)
  );

  const MENU_W = 320;
  let left = $derived(Math.min(x, (typeof window !== 'undefined' ? window.innerWidth : 9999) - MENU_W - 8));
  let top = $derived(
    Math.min(y, (typeof window !== 'undefined' ? window.innerHeight : 9999) - (facts.length * 52 + 44))
  );

  let copiedLabel = $state<string | null>(null);
  let copiedTimer: ReturnType<typeof setTimeout> | null = null;

  async function copyFact(fact: Fact, e: MouseEvent) {
    e.stopPropagation();
    const ok = await copyToClipboard(fact.value);
    if (!ok) return;
    copiedLabel = fact.label;
    if (copiedTimer) clearTimeout(copiedTimer);
    copiedTimer = setTimeout(() => { copiedLabel = null; copiedTimer = null; }, 1200);
    onCopied?.(fact.label);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose();
  }
</script>

<svelte:window
  onclick={onClose}
  oncontextmenu={onClose}
  onscroll={onClose}
  onblur={onClose}
  onkeydown={onKeydown}
/>

<div class="resume-menu" style="left:{left}px; top:{top}px; width:{MENU_W}px;">
  <div class="resume-menu__head">{heading}</div>
  {#each facts as fact (fact.label)}
    <button
      type="button"
      class="resume-menu__row"
      title="Click to copy — {fact.value}"
      onclick={(e) => copyFact(fact, e)}
    >
      <span class="resume-menu__label">
        {fact.label}
        <span class="resume-menu__hint">{copiedLabel === fact.label ? 'Copied ✓' : 'Copy'}</span>
      </span>
      <span class="resume-menu__value" data-copy-text={fact.value}>{fact.value}</span>
    </button>
  {/each}
</div>

<style>
  .resume-menu {
    position: fixed;
    z-index: 1000;
    background: var(--bg-card);
    border: 1px solid var(--border-strong);
    border-radius: 0.4rem;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
    padding: 0.35rem;
    font-size: 0.8rem;
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }
  .resume-menu__head {
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-faint);
    padding: 0.15rem 0.4rem 0.25rem;
  }
  .resume-menu__row {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    text-align: left;
    width: 100%;
    padding: 0.35rem 0.45rem;
    border: 0;
    border-radius: 0.3rem;
    background: none;
    color: var(--text);
    cursor: pointer;
    font-family: inherit;
  }
  .resume-menu__row:hover { background: var(--bg-subtle); }
  .resume-menu__label {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.5rem;
    font-size: 0.72rem;
    color: var(--text-muted);
  }
  .resume-menu__hint { font-size: 0.68rem; color: var(--text-faint); }
  .resume-menu__row:hover .resume-menu__hint { color: var(--accent-user); }
  .resume-menu__value {
    font-family: var(--font-mono, monospace);
    font-size: 0.76rem;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
