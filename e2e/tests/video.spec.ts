import { test, expect } from '@playwright/test';
import { dirname, join } from 'node:path';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { bootstrapSession } from './auth';
import { openDetailsTray, shot } from './helpers';

const HERE = dirname(fileURLToPath(import.meta.url));

test.describe('short video gallery', () => {
  test('shows photos and videos separately and filters by media type', async ({ page }) => {
    await page.goto('/v/clips');
    await expect(page.locator('.image-item')).toHaveCount(4);
    await expect(page.locator('[data-id="clips/scene.png"]')).toBeVisible();
    await expect(page.locator('[data-id="clips/scene.mov"] .video-badge')).toBeVisible();
    await page.getByRole('button', { name: /Videos/ }).click();
    await expect(page.locator('.image-item')).toHaveCount(3);
    await page.getByRole('button', { name: /Photos/ }).click();
    await expect(page.locator('.image-item')).toHaveCount(1);
    await page.getByRole('button', { name: /All/ }).click();
    await expect(page.locator('.image-item')).toHaveCount(4);
    await shot(page, 'video-mixed-grid');
  });

  test('prepares a camera original and plays and seeks in the native player', async ({ page, request }) => {
    test.setTimeout(90_000);
    await expect.poll(async () => {
      const response = await request.get('/api/gallery/video/image/clips/scene.mov');
      return response.ok() ? (await response.json()).image.video?.state : response.status();
    }, { timeout: 60_000 }).toBe('ready');
    await page.goto('/v/detail/clips/scene.mov');
    const player = page.locator('video');
    await expect(player).toBeVisible();
    await expect(player).toHaveAttribute('controls', '');
    await expect(player).toHaveAttribute('playsinline', '');
    await expect(player).toHaveAttribute('preload', 'metadata');
    await expect.poll(() => player.evaluate((video: HTMLVideoElement) => video.readyState)).toBeGreaterThanOrEqual(1);
    await player.evaluate(async (video: HTMLVideoElement) => { video.muted = true; await video.play(); });
    await expect.poll(() => player.evaluate((video: HTMLVideoElement) => video.currentTime)).toBeGreaterThan(0);
    await player.evaluate((video: HTMLVideoElement) => { video.pause(); video.currentTime = 2; });
    await expect.poll(() => player.evaluate((video: HTMLVideoElement) => video.currentTime)).toBeCloseTo(2, 1);
    // Player focus owns arrow keys rather than navigating to another gallery item.
    await player.focus();
    await page.keyboard.press('ArrowRight');
    await expect(page).toHaveURL(/\/v\/detail\/clips\/scene.mov$/);
    await expect(page.locator('.zoom-button, .astro-quick-toggle')).toHaveCount(0);
    await shot(page, 'video-player');
  });

  test('serves byte ranges and HEAD, and enforces visibility and original permissions', async ({ request }) => {
    const response = await request.get('/v/_video/clips/scene.mov', { headers: { Range: 'bytes=0-31' } });
    expect(response.status()).toBe(206);
    expect(response.headers()['content-range']).toMatch(/^bytes 0-31\/\d+$/);
    expect(response.headers()['content-length']).toBe('32');
    expect(response.headers()['content-type']).toBe('video/mp4');
    expect(response.headers()['cache-control']).toContain('private');
    expect((await response.body()).length).toBe(32);
    const etag = response.headers().etag;
    expect((await request.get('/v/_video/clips/scene.mov', { headers: { 'If-None-Match': etag } })).status()).toBe(304);
    const head = await request.head('/v/_video/clips/scene.mov');
    expect(head.status()).toBe(200);
    expect((await head.body()).length).toBe(0);
    expect((await request.get('/v/_video/clips/scene.mov', { headers: { Range: 'bytes=999999999-' } })).status()).toBe(416);
    expect((await request.get('/v/_image/clips/scene.mov')).status()).toBe(403);
    for (const url of ['/v/_video/private/secret.mov', '/v/_video/clips/hidden.mov', '/v/_image/clips/hidden.mov/medium']) {
      expect((await request.get(url)).status(), url).toBe(404);
    }
    const detail = (await (await request.get('/api/gallery/video/image/clips/scene.mov')).json()).image;
    expect(detail.video.codec).toBeUndefined();
    expect(detail.location_info).toBeNull();
    expect(detail.capture_date).not.toContain(' at ');
    const archive = await request.get('/v/_download/archives');
    expect(archive.status()).toBe(200);
    const zip = await archive.body();
    expect(zip.subarray(0, 2).toString()).toBe('PK');
    expect(zip.includes(Buffer.from('visible.mov'))).toBe(true);
    expect(zip.includes(Buffer.from('visible.mov.mov'))).toBe(false);
    expect(zip.includes(Buffer.from('secret.mov'))).toBe(false);
    expect(zip.includes(Buffer.from('hidden.mov'))).toBe(false);
  });

  test('handles portrait and failed clips without image controls', async ({ page }) => {
    test.setTimeout(90_000);
    await page.goto('/v/detail/clips/portrait.mkv');
    await expect(page.locator('video')).toBeVisible({ timeout: 60_000 });
    const dimensions = await page.locator('video').evaluate((video: HTMLVideoElement) => new Promise<[number, number]>((resolve) => {
      const done = () => resolve([video.videoWidth, video.videoHeight]);
      if (video.readyState >= 1) done(); else video.addEventListener('loadedmetadata', done, { once: true });
    }));
    expect(dimensions[1]).toBeGreaterThan(dimensions[0]);
    await page.goto('/v/detail/clips/broken.mov');
    await expect(page.getByText('This video could not be prepared for playback.')).toBeVisible({ timeout: 60_000 });
    await expect(page.locator('video')).toHaveCount(0);
    await shot(page, 'video-failed');
  });

  test('uploads an original through the gallery UI and prepares it asynchronously', async ({ page, context }, testInfo) => {
    test.setTimeout(90_000);
    await bootstrapSession(context);
    await page.goto('/v/uploads');
    await page.getByRole('button', { name: /New/ }).click();
    await page.getByRole('button', { name: /Upload/ }).click();
    const filename = `uploaded-${testInfo.project.name}-${Date.now()}.mov`;
    await page.locator('input[type="file"]').first().setInputFiles({ name: filename, mimeType: 'video/quicktime', buffer: readFileSync(join(HERE, '..', 'fixtures', 'video-photos', 'clips', 'scene.mov')) });
    await page.locator('.uppy-StatusBar-actionBtn--upload').click();
    await page.getByRole('button', { name: 'Done', exact: true }).click();
    await expect(page.locator(`[data-id="uploads/${filename}"]`)).toBeVisible({ timeout: 60_000 });
    await page.locator(`[data-id="uploads/${filename}"] a`).click();
    await expect(page.locator('video')).toBeVisible({ timeout: 60_000 });
    await openDetailsTray(page);
    await expect(page.getByRole('link', { name: 'Download Original' })).toBeVisible();
    await shot(page, 'video-uploaded');
  });

  test('enables and disables video through desktop gallery settings', async ({ page, context, request }) => {
    test.setTimeout(90_000);
    // The existing admin app uses a desktop layout. Keep device-sized viewing
    // and upload coverage in the other scenarios, and exercise settings here.
    await page.setViewportSize({ width: 1280, height: 900 });
    await bootstrapSession(context);
    await page.goto('/_admin/galleries/video');
    const enabled = page.getByRole('checkbox', { name: 'Enable video clips' });
    await expect(enabled).toBeChecked();
    await enabled.uncheck();
    await page.getByRole('button', { name: 'Save Settings' }).click();
    await expect(page.getByText('Gallery settings saved successfully')).toBeVisible();
    await expect.poll(async () => {
      const response = await request.get('/api/gallery/video/data/clips');
      return (await response.json()).video_enabled;
    }).toBe(false);
    const disabled = await (await request.get('/api/gallery/video/data/clips')).json();
    expect(disabled.images.every((image: { media_kind: string }) => image.media_kind !== 'video')).toBe(true);
    expect((await request.get('/api/gallery/video/image/clips/scene.mov')).status()).toBe(404);
    expect((await request.get('/v/_video/clips/scene.mov')).status()).toBe(404);
    await enabled.check();
    await page.getByRole('button', { name: 'Save Settings' }).click();
    await expect.poll(async () => {
      const response = await request.get('/api/gallery/video/image/clips/scene.mov');
      return response.ok() ? (await response.json()).image.video?.state : response.status();
    }, { timeout: 60_000 }).toBe('ready');
    const photoGallery = await request.get('/api/gallery/test/data/by-filename-desc');
    expect(photoGallery.ok()).toBeTruthy();
    expect((await photoGallery.json()).video_enabled).toBe(false);
  });

  test('rejects upload overruns without corrupting the resumable offset, and handles original HEAD', async ({ context }) => {
    await bootstrapSession(context);
    const request = context.request;
    const filename = `bounds-${Date.now()}.mov`;
    const created = await request.post('/_upload/video', { headers: {
      'Tus-Resumable': '1.0.0', 'Upload-Length': '2',
      'Upload-Metadata': `filename ${Buffer.from(filename).toString('base64')},folderPath ${Buffer.from('uploads').toString('base64')}`,
    } });
    expect(created.status()).toBe(201);
    const location = created.headers().location;
    try {
      const overrun = await request.patch(location, { headers: {
        'Tus-Resumable': '1.0.0', 'Upload-Offset': '0', 'Content-Type': 'application/offset+octet-stream',
      }, data: Buffer.from('abc') });
      expect(overrun.status()).toBe(413);
      const state = await request.head(location, { headers: { 'Tus-Resumable': '1.0.0' } });
      expect(state.headers()['upload-offset']).toBe('0');
    } finally {
      await request.delete(location, { headers: { 'Tus-Resumable': '1.0.0' } });
    }
    const original = await request.head('/v/_image/clips/scene.mov', { headers: { Range: 'bytes=0-3' } });
    expect(original.status()).toBe(200);
    expect(original.headers()['content-range']).toBeUndefined();
    expect(Number(original.headers()['content-length'])).toBeGreaterThan(4);
    expect((await original.body()).length).toBe(0);
  });

});
