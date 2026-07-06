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
      const image = item.locator('img.gallery-image-container');
      await expect(image).toHaveAttribute(
        'src',
        /\/_image\/display\/\d{2}-[a-z]+\.png\/thumbnail/,
      );
      await expect(image).toHaveAttribute(
        'srcset',
        /\/_image\/display\/\d{2}-[a-z]+\.png\/thumbnail@2x(?:#retry-\d+)? 2x/,
      );
    }
  });

  test('retries a thumbnail that is still being generated', async ({ page }) => {
    let intercepted = false;
    await page.route(/\/_image\/display\/01-alpha\.png\/thumbnail$/, async (route) => {
      if (!intercepted) {
        intercepted = true;
        await route.fulfill({
          status: 202,
          headers: {
            'cache-control': 'no-store, max-age=0, s-maxage=0',
            'retry-after': '1',
          },
          body: '',
        });
        return;
      }

      await route.continue();
    });

    await openGallery(page, 'display');

    const firstImage = page.locator('.image-grid.square-grid img.gallery-image-container').first();
    await expect(firstImage).toHaveAttribute('data-retry-attempt', /[1-9]\d*/);
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
    const detailImage = page.locator('.image-display img').first();
    await expect(detailImage).toHaveAttribute(
      'src',
      new RegExp(`/_image/by-filename/${first}/medium(?:#retry-\\d+)?$`),
    );
    await page.waitForFunction(() => {
      const image = document.querySelector<HTMLImageElement>('.image-display img');
      return Boolean(image?.complete && image.naturalWidth > 0);
    });
    await shot(page, 'display-detail');
  });
});
