import { test, expect } from '@playwright/test';
import { IMAGE_FILES, openGallery, renderedOrder, shot } from './helpers';

test.describe('image ordering', () => {
  test('filename ascending renders alphabetical order', async ({ page }) => {
    await openGallery(page, 'by-filename');
    await shot(page, 'ordering-filename-asc');

    expect(await renderedOrder(page)).toEqual(IMAGE_FILES);
  });

  test('filename descending renders reverse-alphabetical order', async ({ page }) => {
    await openGallery(page, 'by-filename-desc');
    await shot(page, 'ordering-filename-desc');

    expect(await renderedOrder(page)).toEqual([...IMAGE_FILES].reverse());
  });

  test('descending is the exact reverse of ascending', async ({ page }) => {
    await openGallery(page, 'by-filename');
    const asc = await renderedOrder(page);

    await openGallery(page, 'by-filename-desc');
    const desc = await renderedOrder(page);

    expect(desc).toEqual([...asc].reverse());
  });

  test('custom order honors the configured sequence', async ({ page }) => {
    await openGallery(page, 'custom');
    await shot(page, 'ordering-custom');

    // Matches custom_order in fixtures/photos/custom/_folder.md.
    expect(await renderedOrder(page)).toEqual([
      '03-charlie.png',
      '05-echo.png',
      '01-alpha.png',
      '04-delta.png',
      '02-bravo.png',
    ]);
  });

  test('rendered DOM order matches the server API order', async ({ page, request }) => {
    await openGallery(page, 'by-filename-desc');
    const domOrder = await renderedOrder(page);

    // The server is the source of truth for sort order; confirm React renders
    // images in exactly the order the API returns them.
    const api = await request.get('/api/gallery/test/data/by-filename-desc');
    expect(api.ok()).toBeTruthy();
    const body = await api.json();
    const apiOrder: string[] = (body.images || []).map((img: { path: string }) =>
      img.path.split('/').pop(),
    );

    expect(domOrder).toEqual(apiOrder);
  });
});
