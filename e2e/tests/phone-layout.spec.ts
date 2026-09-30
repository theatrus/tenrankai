import { test, expect, Page } from '@playwright/test';
import { shot } from './helpers';

/**
 * The phone image detail layout, run on the `mobile` (Chromium) and
 * `iphone` (WebKit) projects.
 *
 * iOS Safari applies an `@supports (-webkit-touch-callout: none)` block that
 * gives `.container` a transform. Neither test engine applies it, and it once
 * collapsed the fixed phone layout to nothing, so it is injected here.
 */

const DETAIL = '/g/detail/by-filename/03-charlie.png';

async function openPhoneDetail(page: Page, path = DETAIL) {
  await page.addInitScript(() => {
    document.addEventListener('DOMContentLoaded', () => {
      const style = document.createElement('style');
      style.textContent = '.container, .gallery-container, .image-grid { transform: translate3d(0, 0, 0); }';
      document.head.appendChild(style);
    });
  });
  await page.goto(path);
  await expect(page.locator('.phone-detail')).toBeVisible();
  await expect(page.locator('.phone-stage .image-container img')).toBeVisible();
}

/** Drag the image with synthetic touches (WebKit cannot construct them) */
async function drag(page: Page, from: [number, number], to: [number, number], steps = 8) {
  await page.evaluate(
    async ({ from, to, steps }) => {
      const el = document.querySelector('.phone-stage .swipeable-image-area')!;
      const fire = (type: string, x: number, y: number) => {
        const t = new Touch({ identifier: 1, target: el, clientX: x, clientY: y });
        const list = type === 'touchend' ? [] : [t];
        el.dispatchEvent(
          new TouchEvent(type, { bubbles: true, cancelable: true, touches: list, targetTouches: list, changedTouches: [t] }),
        );
      };
      fire('touchstart', from[0], from[1]);
      for (let i = 1; i <= steps; i++) {
        fire('touchmove', from[0] + ((to[0] - from[0]) * i) / steps, from[1] + ((to[1] - from[1]) * i) / steps);
        await new Promise((r) => setTimeout(r, 16));
      }
      fire('touchend', to[0], to[1]);
    },
    { from, to, steps },
  );
}

test.describe('phone image detail', () => {
  test.beforeEach(({ isMobile }) => {
    test.skip(!isMobile, 'phone layout only');
  });

  test('fills the screen without scrolling, even with the iOS container transform', async ({ page }) => {
    await openPhoneDetail(page);
    const viewport = page.viewportSize()!;

    const stage = (await page.locator('.phone-stage').boundingBox())!;
    expect(stage.height).toBeGreaterThan(viewport.height * 0.4);
    const image = (await page.locator('.phone-stage .image-container').boundingBox())!;
    expect(image.height).toBeGreaterThan(100);

    const tray = (await page.locator('.phone-tray').boundingBox())!;
    expect(Math.round(tray.y + tray.height)).toBe(viewport.height);

    const scroll = await page.evaluate(() => [
      document.documentElement.scrollHeight,
      document.documentElement.scrollWidth,
    ]);
    expect(scroll[0]).toBeLessThanOrEqual(viewport.height);
    expect(scroll[1]).toBeLessThanOrEqual(viewport.width);

    // One top bar replaces the site header and the breadcrumbs
    await expect(page.locator('body > header')).toBeHidden();
    await expect(page.locator('.phone-topbar-back')).toHaveAttribute('href', /\/g\/by-filename#/);
    await shot(page, 'phone-detail');
  });

  test('the menu button shows the site header', async ({ page }) => {
    await openPhoneDetail(page);
    const menu = page.locator('.phone-topbar-menu');
    await menu.click();
    await expect(page.locator('body > header')).toBeVisible();
    // The header drops down below the top bar, so the button can close it
    const header = (await page.locator('body > header').boundingBox())!;
    const bar = (await page.locator('.phone-topbar').boundingBox())!;
    expect(header.y).toBeGreaterThanOrEqual(bar.y + bar.height - 1);
    await menu.click();
    await expect(page.locator('body > header')).toBeHidden();

    await menu.click();
    await page.locator('.phone-menu-backdrop').click({ position: { x: 20, y: 300 } });
    await expect(page.locator('body > header')).toBeHidden();
  });

  test('a tap on the image hides and restores the bars', async ({ page }) => {
    await openPhoneDetail(page);
    const stage = (await page.locator('.phone-stage').boundingBox())!;
    const center: [number, number] = [stage.x + stage.width / 2, stage.y + stage.height / 2];

    await page.touchscreen.tap(...center);
    await expect(page.locator('.phone-detail.immersive')).toHaveCount(1);
    await expect(page.locator('.phone-topbar')).toBeHidden();
    await expect(page.locator('.phone-tray')).toBeHidden();
    await shot(page, 'phone-detail-immersive');

    await page.touchscreen.tap(...center);
    await expect(page.locator('.phone-detail.immersive')).toHaveCount(0);
    await expect(page.locator('.phone-tray')).toBeVisible();
  });

  test('the thumbnail strip navigates in place', async ({ page }) => {
    await openPhoneDetail(page);
    const strip = page.locator('.phone-tray-strip');
    await expect(strip).toBeVisible();
    await expect(strip.locator('.phone-tray-thumb.current')).toHaveCount(1);
    await strip.locator('.phone-tray-thumb:not(.current)').first().click();
    await expect(page).not.toHaveURL(new RegExp(`${DETAIL}$`));
    await expect(page.locator('.phone-detail')).toBeVisible();
  });

  test('dragging follows the finger and navigates', async ({ page, browserName }) => {
    test.skip(browserName === 'webkit', 'WebKit cannot synthesize touch drags');
    await openPhoneDetail(page);
    const stage = (await page.locator('.phone-stage').boundingBox())!;
    const y = stage.y + stage.height / 2;

    // A short drag springs back without navigating
    await drag(page, [stage.x + stage.width * 0.7, y], [stage.x + stage.width * 0.6, y]);
    await expect(page).toHaveURL(new RegExp(`${DETAIL}$`));

    // A long drag to the left moves to the next image
    await drag(page, [stage.x + stage.width * 0.9, y], [stage.x + stage.width * 0.1, y]);
    await expect(page).toHaveURL(/04-delta\.png$/);
    await expect
      .poll(() => page.evaluate(() => document.querySelector<HTMLElement>('.phone-stage .swipeable-image-area')!.style.transform))
      .toBe('');
  });

  test('dragging the image down returns to the folder', async ({ page, browserName }) => {
    test.skip(browserName === 'webkit', 'WebKit cannot synthesize touch drags');
    await openPhoneDetail(page);
    const stage = (await page.locator('.phone-stage').boundingBox())!;
    const x = stage.x + stage.width / 2;
    await drag(page, [x, stage.y + 60], [x, stage.y + stage.height * 0.6]);
    await expect(page).toHaveURL(/\/g\/by-filename#by-filename\/03-charlie\.png$/);
  });
});
