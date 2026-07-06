import { test, expect } from '@playwright/test';

/**
 * Browse view (the home screen) against the bundled mock session — see
 * `src/lib/api.ts`'s browser-dev mock layer and `tests/mock_data/session.jsonl`.
 */
test.describe('Browse view', () => {
  test('loads and shows the one mock session', async ({ page }) => {
    await page.goto('/');

    await expect(page.getByRole('heading', { name: 'CC Deck' })).toBeVisible();

    // Project group for the mock session's cwd ("/dev/mock/demo-project").
    await expect(page.locator('.project-group__name')).toContainText('demo-project');

    // The session card itself: title comes from the first user message.
    const card = page.locator('.session-card');
    await expect(card).toBeVisible();
    await expect(card.locator('.session-card__title')).toHaveText(
      'Show me the current directory structure and explain what this project does.'
    );

    // Stats line: user turn count, subagent count, and model all come from
    // the mock SessionMeta (user_count: 3, subagent_count: 1, models: ['claude-sonnet-4-6']).
    const stats = card.locator('.session-card__stats');
    await expect(stats).toContainText('3 turns');
    await expect(stats).toContainText('1 subagents');
    await expect(stats).toContainText('claude-sonnet-4-6');
  });
});
