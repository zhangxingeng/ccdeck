<script lang="ts">
  /**
   * Notices — the durable trace of data events (contract §S13 / §Store
   * robustness). A repair or an unreadable file flashed a 5s toast when it
   * happened; this section is where it lives on until resolved, because a
   * transient surface must never be the only record of something that touched
   * the user's files. Repaired snippets carry the "open and re-save" nudge;
   * unreadable files carry the parse error. Shown first in the config popover,
   * and the gear wears a badge with the count (§S13).
   */
  import type { Notice } from '$lib/prompts/notices';

  interface Props {
    notices: Notice[];
  }

  let { notices }: Props = $props();
</script>

<section class="notices-sec">
  <div class="config-sec__title">Notices</div>
  <ul class="notices-sec__list">
    {#each notices as n (n.id)}
      <li class="notices-sec__item notices-sec__item--{n.kind}">
        <span class="notices-sec__title" title={n.title}>{n.title}</span>
        <span class="notices-sec__detail">{n.detail}</span>
      </li>
    {/each}
  </ul>
</section>

<style>
  .notices-sec {
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
  .notices-sec__list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .notices-sec__item {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    font-size: 0.72rem;
    border-left: 2px solid var(--accent-template);
    padding: 0.25rem 0 0.25rem 0.5rem;
  }
  .notices-sec__item--unreadable {
    border-left-color: var(--accent-result-err);
  }
  .notices-sec__title {
    font-family: var(--font-mono);
    font-size: 0.68rem;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .notices-sec__detail {
    color: var(--text-muted);
    line-height: 1.45;
  }
</style>
