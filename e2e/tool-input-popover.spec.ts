import { test, expect } from '@playwright/test';

/**
 * Tool input rendering (Block.svelte): collapsed to key chips by default,
 * click opens a popover with the full syntax-highlighted JSON, plus a
 * raw/rendered toggle for long markdown-ish string values. Uses the same
 * Agent tool call as subagent-stack.spec.ts (input keys: description,
 * subagent_type, model, prompt — "prompt" is a long numbered-list string).
 */
test.describe('Tool input popover', () => {
  test('shows key chips, expands to highlighted JSON, and toggles a long field to rendered markdown', async ({ page }) => {
    await page.goto('/');
    await page.locator('.session-card__open').click();
    await expect(page.locator('h2.viewer-title')).toBeVisible();

    const groupToggles = page.locator('.tool-group__toggle');
    const groupCount = await groupToggles.count();
    for (let i = 0; i < groupCount; i++) {
      await groupToggles.nth(i).click();
    }
    // Scope to the Agent tool call specifically — several tool calls in the
    // mock session each render their own .tool-input-chips.
    const agentBlock = page.locator('.msg--tool', { has: page.locator('.collapsible', { hasText: 'Agent' }) });
    await agentBlock.locator('.collapsible', { hasText: 'Agent' }).click();

    // Collapsed input: key-name chips, no raw JSON dumped inline.
    const chips = agentBlock.locator('.tool-input-chips');
    await expect(chips).toBeVisible();
    await expect(chips.locator('.tool-input-chip')).toHaveText(['description', 'subagent_type', 'model', 'prompt']);

    await chips.click();

    // Popover: syntax-highlighted, prettified JSON.
    const modal = page.locator('.tool-input-modal');
    await expect(modal).toBeVisible();
    await expect(modal.locator('.jt-key').first()).toBeVisible();
    await expect(modal.locator('.jt-str', { hasText: 'Explore' })).toBeVisible();

    // The long "prompt" field gets its own raw/rendered toggle.
    const promptSection = modal.locator('.tool-input-longstring', { hasText: 'prompt' });
    await expect(promptSection).toBeVisible();
    await expect(promptSection.locator('pre.tool-json')).toContainText('Is it properly sourced');

    await promptSection.getByRole('button', { name: 'Show rendered' }).click();
    await expect(promptSection.locator('li').first()).toContainText('Is it properly sourced');
    await expect(promptSection.getByRole('button', { name: 'Show raw' })).toBeVisible();

    await modal.getByRole('button', { name: 'Close' }).click();
    await expect(modal).toHaveCount(0);
  });
});
