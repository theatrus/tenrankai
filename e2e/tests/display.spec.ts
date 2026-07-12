import { test, expect } from '@playwright/test';
import { bootstrapSession } from './auth';
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

// Sky position is gated behind can_see_technical_details, which anonymous
// visitors lack by default — run authenticated
test.describe('astro sky map (authenticated)', () => {
  test.beforeEach(async ({ context }) => {
    await bootstrapSession(context);
  });

  test('astro images show a sky map locating their coordinates', async ({ page }) => {
    // 03-charlie has a .md sidecar with telescope + RA/Dec metadata (M1)
    await page.goto('/g/detail/by-filename/03-charlie.png');

    const skyMap = page.locator('.sky-map');
    await expect(skyMap).toBeVisible();
    await expect(skyMap.locator('h3')).toHaveText('Sky Position');
    await expect(skyMap.locator('.coordinates-text')).toContainText('05h 34m 32s');

    // The chart renders the star field and the target crosshair
    const chart = skyMap.locator('.sky-map-chart');
    await expect(chart).toBeVisible();
    expect(await chart.locator('.sky-map-star').count()).toBeGreaterThan(400);
    await expect(chart.locator('circle.sky-map-target')).toHaveCount(1);

    // External viewer links carry the decimal coordinates
    const aladin = skyMap.locator('a', { hasText: 'Aladin Lite' });
    expect(await aladin.getAttribute('href')).toContain(
      encodeURIComponent('83.6333 +22.0144'),
    );
    await expect(skyMap.locator('a', { hasText: 'SIMBAD' })).toBeVisible();
    await shot(page, 'image-detail-sky-map');

    // Images without RA/Dec metadata have no sky map
    await page.goto('/g/detail/by-filename/01-alpha.png');
    await expect(page.locator('.image-metadata')).toBeVisible();
    await expect(page.locator('.sky-map')).toHaveCount(0);
  });
});
