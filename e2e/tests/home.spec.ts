import { test, expect } from '@playwright/test';
import { shot } from './helpers';

test.describe('home page', () => {
  test('hides the gallery preview when preview data is unavailable', async ({ page }) => {
    await page.goto('/');

    const preview = page.locator('#gallery-preview-component');
    await expect(preview).toBeHidden();
    await expect(preview.locator('.preview-item')).toHaveCount(0);
    await shot(page, 'home-gallery-preview');
  });
});
