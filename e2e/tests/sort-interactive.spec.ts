import { test, expect } from '@playwright/test';
import {
  ADMIN_USER,
  bootstrapSession,
  clearSession,
  enableVirtualAuthenticator,
  loginWithPasskeyViaUI,
  registerPasskeyViaUI,
} from './auth';
import { openGallery, renderedOrder, shot } from './helpers';

test.describe('interactive sort-order change', () => {
  test('admin flips sort direction in the UI and the grid reorders + persists', async ({
    page,
    context,
  }) => {
    // Sign in as the admin via passkey (registered through the real UI), landing
    // on the manage folder.
    await enableVirtualAuthenticator(page);
    await bootstrapSession(context);
    await registerPasskeyViaUI(page);
    await page.goto('/_login/logout').catch(() => {});
    await clearSession(context);
    await loginWithPasskeyViaUI(page, ADMIN_USER, '/g/manage');

    await openGallery(page, 'manage');
    const initialOrder = await renderedOrder(page);
    await shot(page, 'manage-before');

    // Enter manage mode -> the owner toolbar with the sort control appears.
    await page.locator('#manage-images-btn').click();
    const directionSelect = page.locator('.sort-order-control .sort-order-select').nth(1);
    await expect(directionSelect).toBeVisible();

    // Flip to the opposite direction (order-agnostic, so retries are safe) and
    // confirm the control persisted it through the admin API.
    const current = await directionSelect.inputValue();
    const target = current === 'asc' ? 'desc' : 'asc';
    const [putResponse] = await Promise.all([
      page.waitForResponse(
        (r) => r.url().includes('/sort-order') && r.request().method() === 'PUT',
      ),
      directionSelect.selectOption(target),
    ]);
    expect(putResponse.ok()).toBeTruthy();

    // The control refetches and re-renders the grid in place (no full reload) to
    // the new server order — the exact reverse of the starting order.
    const reversed = [...initialOrder].reverse();
    await expect
      .poll(async () => (await renderedOrder(page)).join(','), { timeout: 15_000 })
      .toBe(reversed.join(','));
    await shot(page, 'manage-after');

    // Persisted: a fresh load (no manage interaction) still shows the new order.
    await openGallery(page, 'manage');
    expect(await renderedOrder(page)).toEqual(reversed);
  });
});
