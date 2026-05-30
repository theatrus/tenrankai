import { BrowserContext, CDPSession, Page, expect } from '@playwright/test';
import { createHmac } from 'node:crypto';

// Must match e2e/fixtures/config.toml [app] cookie_secret and the seeded user.
const COOKIE_SECRET = 'e2e-test-cookie-secret-not-a-real-secret';
export const ADMIN_USER = 'e2euser';

/**
 * Recreate the server's signed session cookie: `username:HMAC_SHA256_b64url`.
 * See tenrankai::api::create_signed_cookie. Used only to bootstrap the very
 * first session so a passkey can be registered (registration requires auth).
 */
function signedCookieValue(username: string): string {
  const sig = createHmac('sha256', COOKIE_SECRET).update(username).digest('base64url');
  return `${username}:${sig}`;
}

/** Inject a valid admin session cookie (the `auth` cookie) into the context. */
export async function bootstrapSession(context: BrowserContext, username = ADMIN_USER) {
  await context.addCookies([
    {
      name: 'auth',
      value: signedCookieValue(username),
      domain: 'localhost',
      path: '/',
      httpOnly: true,
      sameSite: 'Lax',
    },
  ]);
}

export async function clearSession(context: BrowserContext) {
  await context.clearCookies({ name: 'auth' });
}

/**
 * Attach a Chromium WebAuthn virtual authenticator to the page so
 * navigator.credentials.create()/get() resolve without hardware. Keep using
 * the SAME page afterwards — the authenticator (and its credentials) lives on
 * this page's CDP target.
 */
export async function enableVirtualAuthenticator(page: Page): Promise<CDPSession> {
  const client = await page.context().newCDPSession(page);
  await client.send('WebAuthn.enable');
  await client.send('WebAuthn.addVirtualAuthenticator', {
    options: {
      protocol: 'ctap2',
      transport: 'internal',
      hasResidentKey: true,
      hasUserVerification: true,
      isUserVerified: true,
      automaticPresenceSimulation: true,
    },
  });
  return client;
}

/**
 * Register a passkey through the real enrollment UI. Requires an authenticated
 * session (use bootstrapSession first) and an enabled virtual authenticator.
 */
export async function registerPasskeyViaUI(page: Page) {
  await page.goto('/_login/passkey-enrollment?return=/_login/profile');
  await page.locator('#setupPasskeyBtn').click();
  // On success the page redirects to the return URL (the profile page), which
  // lists the registered passkey. (Other tests share the seeded user, so assert
  // "at least one" rather than an exact count.)
  await page.waitForURL('**/_login/profile', { timeout: 15_000 });
  await expect(page.locator('#passkeyList .passkey-item').first()).toBeVisible();
}

/**
 * Drive the real login UI with a passkey: enter the username, submit, and let
 * the page's WebAuthn flow authenticate via the virtual authenticator.
 * `returnTo` becomes the post-login destination.
 */
export async function loginWithPasskeyViaUI(page: Page, username = ADMIN_USER, returnTo = '/') {
  await page.goto(`/_login?return=${encodeURIComponent(returnTo)}`);
  await page.locator('#username').fill(username);
  await page.locator('#loginForm button[type="submit"]').click();
  // handleLoginSuccess redirects ~1s after the assertion verifies.
  await page.waitForURL(
    (url) => !url.pathname.startsWith('/_login'),
    { timeout: 15_000 },
  );
}
