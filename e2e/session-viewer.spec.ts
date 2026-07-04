import { test, expect } from '@playwright/test';

/**
 * Opening the mock session and navigating back. See
 * `tests/mock_data/session.jsonl` for the underlying conversation.
 */
test.describe('Session viewer', () => {
  test('opening the mock session shows its messages', async ({ page }) => {
    await page.goto('/');
    await page.locator('.session-card__open').click();

    // Viewer title mirrors the session's derived title (first user message,
    // no custom_title set in the mock meta).
    await expect(page.locator('h2.viewer-title')).toHaveText(
      'Show me the current directory structure and explain what this project does.'
    );

    // A known assistant reply and the final (interrupted) user turn both render
    // as chat bubbles.
    await expect(page.getByText('full-stack task workflow manager')).toBeVisible();
    await expect(page.getByText('Yes please fix it.')).toBeVisible();
  });

  test('the ← Back button returns to Browse', async ({ page }) => {
    await page.goto('/');
    await page.locator('.session-card__open').click();
    await expect(page.locator('h2.viewer-title')).toBeVisible();

    await page.getByRole('button', { name: '← Back' }).click();

    await expect(page.locator('.session-card')).toBeVisible();
    await expect(page.locator('h2.viewer-title')).toHaveCount(0);
  });
});
