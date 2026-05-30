import { test, expect } from '@playwright/test';
import { IMAGE_FILES, openGallery, renderedOrder, shot } from './helpers';

test.describe('gallery display', () => {
  test('renders every image in a square grid', async ({ page }) => {
    await openGallery(page, 'display');
    await shot(page, 'display-grid');

    const items = page.locator('.image-grid.square-grid .image-item');
    await expect(items).toHaveCount(IMAGE_FILES.length);

    // Each item links to its detail page and paints a thumbnail.
    for (let i = 0; i < IMAGE_FILES.length; i++) {
      const item = items.nth(i);
      await expect(item.locator('a.image-link')).toHaveAttribute(
        'href',
        /\/g\/detail\/display\/\d{2}-[a-z]+\.png$/,
      );
      const bg = await item
        .locator('.gallery-image-container')
        .evaluate((el) => getComputedStyle(el).backgroundImage);
      expect(bg).toMatch(/\/_image\/display\/\d{2}-[a-z]+\.png\/thumbnail/);
    }
  });

  test('thumbnails are served successfully', async ({ page }) => {
    const failed: string[] = [];
    page.on('response', (res) => {
      if (res.url().includes('/_image/') && res.status() >= 400) {
        failed.push(`${res.status()} ${res.url()}`);
      }
    });

    await openGallery(page, 'display');
    expect(failed, `thumbnail requests failed:\n${failed.join('\n')}`).toEqual([]);
  });

  test('clicking an image opens its detail page', async ({ page }) => {
    await openGallery(page, 'by-filename');
    const first = (await renderedOrder(page))[0];

    await page.locator('.image-grid.square-grid .image-item').first().locator('a.image-link').click();

    await expect(page).toHaveURL(new RegExp(`/g/detail/by-filename/${first}$`));
    await shot(page, 'display-detail');
  });
});
