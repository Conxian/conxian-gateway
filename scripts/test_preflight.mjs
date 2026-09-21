import { execSync } from 'child_process';

console.log('[preflight] Running E2E test preflight checks...');

// 1. Check NEXTAUTH_SECRET presence
if (!process.env.NEXTAUTH_SECRET) {
  console.log('[preflight] NEXTAUTH_SECRET is missing. Setting default sentinel secret for local E2E run...');
  process.env.NEXTAUTH_SECRET = 'sentinel_nextauth_secret';
} else {
  console.log('[preflight] NEXTAUTH_SECRET is set.');
}

// 2. Ensure Playwright Chromium browser is installed
console.log('[preflight] Verifying Playwright Chromium browser engine installation...');
try {
  execSync('pnpm --filter control-plane exec playwright install chromium', {
    stdio: 'inherit',
    env: process.env,
  });
  console.log('[preflight] Playwright Chromium installation check passed.');
} catch (err) {
  console.error('[preflight] Failed to verify/install Playwright Chromium browser engine:', err);
  process.exit(1);
}

// 3. Execute test suite (Vitest + Playwright)
console.log('[preflight] Preflight checks complete. Executing test suite (pnpm test)...');
try {
  execSync('pnpm test', {
    stdio: 'inherit',
    env: process.env,
  });
} catch (err) {
  console.error('[preflight] Test suite execution failed.');
  process.exit(err.status || 1);
}
