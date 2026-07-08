<script lang="ts">
  /**
   * Turn.svelte — renders a single conversation turn.
   *
   * Iterates blocks via Block.svelte.  If turn.isInterrupted, appends
   * an .interrupt-banner after the blocks.
   */
  import type { Turn } from '$lib/types';
  import Block from './Block.svelte';

  let { turn }: { turn: Turn } = $props();
</script>

{#each turn.blocks as block, i (i)}
  <Block {block} role={turn.role} />
{/each}

{#if turn.isInterrupted}
  <div class="interrupt-banner">Interrupted by user</div>
{/if}
