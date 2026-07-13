import { test, expect, Page } from '@playwright/test';
import { shot } from './helpers';

/**
 * Astro overlay controls, exercised on desktop and touch devices (the
 * `mobile` Playwright project). The plate-solve API is mocked so the
 * fixture gallery needs no star catalogs or real solving.
 */

const SOLUTION = {
  solved: true,
  width: 800,
  height: 600,
  center: { ra: 10.68, dec: 41.27 },
  scale_arcsec_px: 2.58,
  matched_stars: 42,
  rms_arcsec: 1.1,
  objects: [
    {
      name: 'NGC 224',
      common_name: 'Andromeda Galaxy',
      kind: 'galaxy',
      mag: 3.4,
      x: 400,
      y: 300,
      semi_major_px: 120,
      semi_minor_px: 60,
      angle_deg: 35,
    },
    {
      name: 'WR 134',
      common_name: 'V1769 Cyg',
      kind: 'star',
      mag: 8.1,
      x: 200,
      y: 150,
      semi_major_px: 0,
      semi_minor_px: 0,
      angle_deg: 0,
    },
    {
      name: 'SN 2026sqf',
      common_name: 'type II, disc. 2026/07/08',
      kind: 'transient',
      mag: 12.7,
      x: 600,
      y: 420,
      semi_major_px: 0,
      semi_minor_px: 0,
      angle_deg: 0,
      discovered: '2026-07-08',
      near_capture: true,
    },
    {
      name: 'C/2025 A6 (Lemmon)',
      common_name: 'V~7.6, 0.71 AU',
      kind: 'comet',
      mag: 7.6,
      x: 300,
      y: 450,
      semi_major_px: 0,
      semi_minor_px: 0,
      angle_deg: 0,
      near_capture: true,
    },
    {
      name: 'SN Nova M31 2022-10a',
      common_name: 'disc. 2022/10/26',
      kind: 'transient',
      mag: 17.0,
      x: 500,
      y: 200,
      semi_major_px: 0,
      semi_minor_px: 0,
      angle_deg: 0,
      discovered: '2022-10-26',
      near_capture: false,
    },
  ],
};

async function openSolvedDetail(page: Page) {
  await page.route('**/api/gallery/*/astro/**', (route) =>
    route.fulfill({ json: SOLUTION }),
  );
  await page.goto('/g/by-filename');
  await page
    .locator('.image-grid.square-grid .image-item')
    .first()
    .locator('a.image-link')
    .click();
  await expect(page).toHaveURL(/\/g\/detail\//);
}

test.describe('astro overlay', () => {
  test('Objects toggle reveals the overlay and old transients stay hidden', async ({
    page,
  }) => {
    await openSolvedDetail(page);

    const toggle = page.getByRole('button', { name: /Objects \(/ });
    await expect(toggle).toBeVisible();
    // Count excludes the one out-of-window transient by default
    await expect(toggle).toHaveText('Objects (4)');
    await toggle.click();

    const svg = page.locator('svg[aria-label="Sky object overlay"]');
    await expect(svg).toBeVisible();
    await expect(svg.getByText('NGC 224 · Andromeda Galaxy')).toBeVisible();
    await expect(svg.getByText(/SN 2026sqf/)).toBeVisible();
    await expect(svg.getByText(/Lemmon/)).toBeVisible();
    await expect(svg.getByText(/2022-10a/)).toHaveCount(0);
    await shot(page, 'astro-overlay-visible');

    // The old-transient toggle brings the historical nova back
    const older = page.getByRole('button', { name: /old transients/ });
    await expect(older).toHaveText('+1 old transients');
    await older.click();
    await expect(svg.getByText(/2022-10a/)).toBeVisible();
    await shot(page, 'astro-overlay-all-transients');

    // And the main toggle hides everything again
    await page.getByRole('button', { name: 'Objects ✕' }).click();
    await expect(svg).toHaveCount(0);
  });

  test('toggles work with taps on touch devices', async ({ page, isMobile }) => {
    test.skip(!isMobile, 'touch-specific interaction');
    await openSolvedDetail(page);

    const toggle = page.getByRole('button', { name: /Objects \(/ });
    await expect(toggle).toBeVisible();
    // Tap target must meet the 44-px minimum
    const box = await toggle.boundingBox();
    expect(box!.height).toBeGreaterThanOrEqual(44);

    await toggle.tap();
    const svg = page.locator('svg[aria-label="Sky object overlay"]');
    await expect(svg).toBeVisible();
    await shot(page, 'astro-overlay-mobile');

    // Tapping the toggle must not have opened the mobile zoom dialog
    await expect(page.locator('.image-display')).toBeVisible();

    const older = page.getByRole('button', { name: /old transients/ });
    await older.tap();
    await expect(svg.getByText(/2022-10a/)).toBeVisible();

    await page.getByRole('button', { name: 'Objects ✕' }).tap();
    await expect(svg).toHaveCount(0);
  });

  test('mobile zoom carries the overlay with the image', async ({ page, isMobile }) => {
    test.skip(!isMobile, 'touch-specific interaction');
    await openSolvedDetail(page);

    await page.getByRole('button', { name: /Objects \(/ }).tap();
    const svg = page.locator('svg[aria-label="Sky object overlay"]');
    await expect(svg.first()).toBeVisible();

    // Double-tap opens the mobile zoom view; the normal image view
    // unmounts, so any overlay still present is the one inside the zoomed
    // scaling container — it pans and zooms with the image
    // Two raw taps inside the 300 ms double-tap window
    const box = (await page.locator('.image-display').boundingBox())!;
    const [cx, cy] = [box.x + box.width / 2, box.y + box.height / 2];
    await page.touchscreen.tap(cx, cy);
    await page.touchscreen.tap(cx, cy);
    await expect(page.getByRole('button', { name: '×' })).toBeVisible();
    await expect(svg.first()).toBeVisible();
    await expect(svg.first().getByText('NGC 224 · Andromeda Galaxy')).toBeVisible();
    // The zoom must STAY open: a stale close-timeout from the tap handler
    // used to stomp it shut ~300ms after opening
    await page.waitForTimeout(800);
    await expect(page.getByRole('button', { name: '×' })).toBeVisible();
    await expect(svg.first().getByText('NGC 224 · Andromeda Galaxy')).toBeVisible();
    await shot(page, 'astro-overlay-mobile-zoom');
  });
});
