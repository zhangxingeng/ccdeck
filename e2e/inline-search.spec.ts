import { test, expect } from '@playwright/test';

/**
 * "Find in this chat" (InlineSearchPanel.svelte), opened with Ctrl/Cmd+F from
 * inside an open session — see SessionEditor.svelte's keydown handler.
 */
test.describe('Find in chat', () => {
  test('Ctrl+F opens the panel and searching highlights a hit', async ({ page }) => {
    await page.goto('/');
    await page.locator('.session-card__open').click();
    await expect(page.locator('h2.viewer-title')).toBeVisible();

    await page.keyboard.press('Control+f');

    const searchInput = page.locator('.ics__input input[type="text"]');
    await expect(searchInput).toBeVisible();
    await expect(searchInput).toBeFocused();

    // "SECRET_KEY" appears verbatim in one of the mock session's user turns.
    await searchInput.fill('SECRET_KEY');

    await expect(page.locator('.ics__status')).toContainText('match');
    await expect(page.locator('.ics-hit').first()).toBeVisible();
    await expect(page.locator('.ics-hit mark').first()).toHaveText('SECRET_KEY');
  });
});
