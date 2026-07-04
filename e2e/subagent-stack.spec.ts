import { test, expect } from '@playwright/test';

/**
 * Subagent stacked navigation: clicking a tool_use's "Open →" affordance
 * (Block.svelte) pushes onto SessionEditor's subagentStack and renders the
 * nested transcript read-only; the header's ← Back / Esc pop one level.
 * See tests/mock_data/subagents/agent-audit-secret.jsonl for the fixture.
 */
test.describe('Subagent stacked navigation', () => {
  test('opening a subagent shows its transcript, Esc pops back to the main session', async ({ page }) => {
    await page.goto('/');
    await page.locator('.session-card__open').click();
    await expect(page.locator('h2.viewer-title')).toBeVisible();

    // Expand every collapsed tool-group so the Agent tool call's block is reachable.
    const groupToggles = page.locator('.tool-group__toggle');
    const groupCount = await groupToggles.count();
    for (let i = 0; i < groupCount; i++) {
      await groupToggles.nth(i).click();
    }

    // Expand the Agent tool_use block itself to reveal the subagent affordance.
    await page.locator('.collapsible', { hasText: 'Agent' }).click();

    const openBtn = page.locator('.subagent-open');
    await expect(openBtn).toBeVisible();
    await expect(openBtn).toContainText('Audit SECRET_KEY usage');
    await openBtn.click();

    // Stacked view: breadcrumbs + the subagent's own read-only turns.
    await expect(page.locator('.subagent-crumbs')).toBeVisible();
    await expect(page.locator('.subagent-crumbs__trail')).toContainText('Audit SECRET_KEY usage');
    await expect(page.getByText('Test fixture properly uses env override')).toBeVisible();

    // Esc pops back to the main session (not out of the viewer entirely).
    await page.keyboard.press('Escape');
    await expect(page.locator('.subagent-crumbs')).toHaveCount(0);
    await expect(page.locator('h2.viewer-title')).toBeVisible();
  });

  test('the header ← Back pops one subagent level instead of leaving the viewer', async ({ page }) => {
    await page.goto('/');
    await page.locator('.session-card__open').click();

    const groupToggles = page.locator('.tool-group__toggle');
    const groupCount = await groupToggles.count();
    for (let i = 0; i < groupCount; i++) {
      await groupToggles.nth(i).click();
    }
    await page.locator('.collapsible', { hasText: 'Agent' }).click();
    await page.locator('.subagent-open').click();
    await expect(page.locator('.subagent-crumbs')).toBeVisible();

    await page.getByRole('button', { name: '← Back' }).click();

    // Still in the viewer (on the main session), not back at Browse.
    await expect(page.locator('.subagent-crumbs')).toHaveCount(0);
    await expect(page.locator('h2.viewer-title')).toBeVisible();
    await expect(page.locator('.session-card')).toHaveCount(0);
  });
});
