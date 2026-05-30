import { test, expect } from '@playwright/test';
import {
  ADMIN_USER,
  bootstrapSession,
  clearSession,
  enableVirtualAuthenticator,
  loginWithPasskeyViaUI,
  registerPasskeyViaUI,
} from './auth';
import { shot } from './helpers';

test.describe('passkey login', () => {
  test('register a passkey, then sign in with it through the UI', async ({ page, context }) => {
    await enableVirtualAuthenticator(page);

    // Registration requires an authenticated session, so mint one to bootstrap,
    // then register a passkey through the real enrollment UI.
    await bootstrapSession(context);
    await registerPasskeyViaUI(page);
    await shot(page, 'passkey-registered');

    // Drop the bootstrap session entirely — the next sign-in must succeed purely
    // via the passkey.
    await page.goto('/_login/logout').catch(() => {});
    await clearSession(context);

    // Drive the real login form; the virtual authenticator answers the WebAuthn
    // challenge.
    await loginWithPasskeyViaUI(page, ADMIN_USER, '/');
    await shot(page, 'passkey-logged-in');

    // The server minted a fresh, valid admin session.
    const verify = await page.request.get('/api/verify');
    expect(verify.ok()).toBeTruthy();
    expect((await verify.json()).is_admin).toBe(true);

    // And the passkey is listed on the profile page.
    await page.goto('/_login/profile');
    await expect(page.locator('#passkeyList .passkey-item').first()).toBeVisible();
  });
});
