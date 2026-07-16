import { test, expect } from '@playwright/test';

/**
 * Fork-from-here (issue #34). The ⑂ affordance on each message forks the
 * session and opens ResumeMenu on the NEW session's copyable resume facts.
 *
 * This exists because fork shipped with zero E2E coverage and a regression rode
 * in unnoticed: ResumeMenu mounts a `<svelte:window onclick={onClose}>`
 * outside-click guard, and the opening click self-closed the popover before
 * paint (the button looked dead) until the handler learned to stopPropagation.
 * The browser-dev mock layer (src/lib/api.ts) returns a mock fork id, so this
 * runs against the dev server with no Tauri backend.
 */
test.describe('Fork-from-here (#34)', () => {
  test('fork opens the resume popover and it stays open', async ({ page }) => {
    await page.goto('/');
    await page.locator('.session-card__open').click();
    await expect(page.locator('h2.viewer-title')).toBeVisible();

    const group = page.locator('.msg-group').first();
    await group.hover();
    const forkBtn = group.locator('.msg-tools__btn').first();
    await expect(forkBtn).toHaveText('⑂');
    await forkBtn.click();

    // The popover appears — and, crucially, is still there after the click
    // settles (the self-close regression made it vanish immediately).
    const menu = page.locator('.resume-menu');
    await expect(menu).toBeVisible();
    await expect(menu).toContainText('claude --resume');
    await page.waitForTimeout(300);
    await expect(menu).toBeVisible();
  });

  test('the fork affordance is hidden in select mode', async ({ page }) => {
    await page.goto('/');
    await page.locator('.session-card__open').click();
    await expect(page.locator('h2.viewer-title')).toBeVisible();

    await expect(page.locator('.msg-group').first().locator('.msg-tools')).toHaveCount(1);
    await page.getByRole('button', { name: '☑ Select' }).click();
    await expect(page.locator('.msg-group').first().locator('.msg-tools')).toHaveCount(0);
  });
});
