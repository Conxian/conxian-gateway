import { test, expect } from '@playwright/test';

test('redirects unauthenticated users to signin', async ({ page }) => {
  await page.goto('/');
  // Unauthenticated access redirects to GitHub OAuth signin
  await expect(page.getByText('Sign in with GitHub')).toBeVisible();
});

test('auth signin page renders', async ({ page }) => {
  await page.goto('/auth/signin');
  await expect(page.getByRole('button', { name: /Sign in with GitHub/i })).toBeVisible();
});
