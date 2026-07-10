<script lang="ts">
  /**
   * Shortcuts — the hotkey rebinding UI (contract §Hotkey map / JC-8). One row
   * per rebindable command: its current chord as a key-cap, `Change` (capture
   * the next chord), `Reset` (drop the override). A chord already bound to
   * another command is rejected inline — we never silently steal a binding.
   * `Esc` abandons capture. A chord that collides with a browser/OS default is
   * allowed but flagged "overrides system default". Minimal by design — no
   * conflict graph, no import/export until someone wants one.
   *
   * Only *command* hotkeys rebind; ↓ / Enter / Esc are spatial/context keys,
   * not commands, and never appear here.
   */
  import { prompts, resolvedHotkeys, setHotkey, resetHotkey } from '$lib/prompts.svelte';
  import {
    HOTKEY_COMMANDS,
    HOTKEY_LABELS,
    chordFromEvent,
    findConflict,
    overridesSystem,
    type HotkeyCommand,
  } from '$lib/prompts/hotkeys';

  const hotkeys = $derived(resolvedHotkeys());
  let capturing = $state<HotkeyCommand | null>(null);
  let rejection = $state<string | null>(null);

  function startCapture(command: HotkeyCommand): void {
    rejection = null;
    capturing = command;
  }

  function cancelCapture(): void {
    capturing = null;
    rejection = null;
  }

  function onCaptureKeydown(e: KeyboardEvent): void {
    if (!capturing) return;
    // Own the keyboard entirely while capturing — no chord should leak to the
    // page (or activate this button) mid-capture.
    e.preventDefault();
    e.stopPropagation();
    if (e.key === 'Escape') {
      cancelCapture(); // abandon (contract: Esc abandons the capture)
      return;
    }
    const chord = chordFromEvent(e);
    if (!chord) return; // a bare modifier — keep waiting for the full chord
    const conflict = findConflict(hotkeys, capturing, chord);
    if (conflict) {
      // Rejected inline; nothing stored; stay in capture so the user can retry.
      rejection = `${chord} is already ${HOTKEY_LABELS[conflict]}.`;
      return;
    }
    const command = capturing;
    capturing = null;
    rejection = null;
    void setHotkey(command, chord);
  }

  function reset(command: HotkeyCommand): void {
    if (capturing === command) cancelCapture();
    void resetHotkey(command);
  }
</script>

<section class="shortcuts-sec">
  <div class="config-sec__title">Shortcuts</div>
  {#each HOTKEY_COMMANDS as command (command)}
    <div class="shortcuts-sec__row">
      <span class="shortcuts-sec__label">{HOTKEY_LABELS[command]}</span>
      {#if capturing === command}
        <button
          type="button"
          class="shortcuts-sec__cap"
          onkeydown={onCaptureKeydown}
          {@attach (node) => void node.focus()}
        >
          press a key… (Esc to cancel)
        </button>
      {:else}
        <kbd class="shortcuts-sec__chord">{hotkeys[command]}</kbd>
        {#if overridesSystem(hotkeys[command])}
          <span class="shortcuts-sec__sys" title="Overrides a browser/OS default; effective because the binding preventDefaults">overrides system default</span>
        {/if}
      {/if}
      <span class="shortcuts-sec__spacer"></span>
      <button
        type="button"
        class="btn btn--ghost btn--sm"
        onclick={() => (capturing === command ? cancelCapture() : startCapture(command))}
      >
        {capturing === command ? 'Cancel' : 'Change'}
      </button>
      <button
        type="button"
        class="btn btn--ghost btn--sm"
        disabled={!prompts.hotkeyOverrides[command]}
        onclick={() => reset(command)}
      >
        Reset
      </button>
    </div>
  {/each}

  {#if rejection}<p class="shortcuts-sec__msg">{rejection}</p>{/if}
  {#if prompts.configError}<p class="shortcuts-sec__msg">{prompts.configError}</p>{/if}
</section>

<style>
  .shortcuts-sec {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .config-sec__title {
    font-size: 0.66rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-faint);
  }
  .shortcuts-sec__row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.72rem;
  }
  .shortcuts-sec__label {
    color: var(--text);
  }
  .shortcuts-sec__spacer {
    flex: 1;
  }
  .shortcuts-sec__chord,
  .shortcuts-sec__cap {
    font-family: var(--font-mono);
    font-size: 0.68rem;
    padding: 0.15rem 0.45rem;
    border: 1px solid var(--border);
    border-radius: 0.3rem;
    background: var(--bg-subtle);
    color: var(--text);
  }
  .shortcuts-sec__cap {
    cursor: text;
    color: var(--accent-snippet);
    border-color: color-mix(in srgb, var(--accent-snippet) 55%, var(--border));
  }
  .shortcuts-sec__sys {
    font-size: 0.6rem;
    color: var(--accent-template);
  }
  .shortcuts-sec__msg {
    font-size: 0.68rem;
    color: var(--accent-result-err);
    margin: 0;
  }
</style>
