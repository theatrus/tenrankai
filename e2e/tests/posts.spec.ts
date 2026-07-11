import { test, expect } from '@playwright/test';
import { shot } from './helpers';

// Fixture posts (e2e/fixtures/posts), newest first:
//   Plain Note   2024-01-04  (no categories)
//   Camera Bag   2024-01-03  (Gear, hero from first content image)
//   Second Trip  2024-01-02  (Travel, Gear)
//   First Trip   2024-01-01  (Travel, explicit gallery hero)
// posts_per_page = 3, so the unfiltered index has two pages.

test.describe('posts index', () => {
  test('shows post cards newest-first with metadata', async ({ page }) => {
    await page.goto('/blog');

    const cards = page.locator('.post-card');
    await expect(cards).toHaveCount(3);
    await expect(cards.nth(0).locator('h2')).toHaveText('Plain Note');
    await expect(cards.nth(1).locator('h2')).toHaveText('Camera Bag');
    await expect(cards.nth(2).locator('h2')).toHaveText('Second Trip');

    // First card on the first page is featured
    await expect(cards.nth(0)).toHaveClass(/featured/);

    // Date and reading time
    await expect(cards.nth(0).locator('.post-card-meta')).toContainText('January 4, 2024');
    await expect(cards.nth(0).locator('.post-card-meta')).toContainText('min read');

    await shot(page, 'posts-index');
  });

  test('shows category chips with counts', async ({ page }) => {
    await page.goto('/blog');

    const chips = page.locator('.category-bar .category-chip');
    await expect(chips).toHaveCount(4); // All, Gear, Travel, RSS
    await expect(chips.nth(0)).toHaveText('All');
    await expect(chips.nth(0)).toHaveClass(/active/);
    await expect(chips.nth(1)).toContainText('Gear');
    await expect(chips.nth(1).locator('.category-count')).toHaveText('2');
    await expect(chips.nth(2)).toContainText('Travel');
    await expect(chips.nth(2).locator('.category-count')).toHaveText('2');

    const rss = page.locator('.category-bar .feed-chip');
    expect(await rss.getAttribute('href')).toBe('/blog/feed.xml');
  });

  test('renders hero images from gallery references', async ({ page }) => {
    await page.goto('/blog');

    // Camera Bag's hero falls back to the first content image (a gallery ref)
    const cameraBag = page.locator('.post-card', { hasText: 'Camera Bag' });
    const heroImg = cameraBag.locator('.post-card-image img');
    await expect(heroImg).toBeVisible();
    expect(await heroImg.getAttribute('src')).toContain('/g/_image/');

    // Plain Note has no images at all
    const plainNote = page.locator('.post-card', { hasText: 'Plain Note' });
    await expect(plainNote.locator('.post-card-image')).toHaveCount(0);
  });

  test('filters posts by category', async ({ page }) => {
    await page.goto('/blog');
    await page.locator('.category-bar .category-chip', { hasText: 'Travel' }).click();

    await expect(page).toHaveURL(/\/blog\/category\/travel$/);
    const cards = page.locator('.post-card');
    await expect(cards).toHaveCount(2);
    await expect(cards.nth(0).locator('h2')).toHaveText('Second Trip');
    await expect(cards.nth(1).locator('h2')).toHaveText('First Trip');

    // Active chip + filter note with a way back to the full index
    await expect(
      page.locator('.category-bar .category-chip', { hasText: 'Travel' }),
    ).toHaveClass(/active/);
    await expect(page.locator('.posts-filter-note')).toContainText('Travel');
    await shot(page, 'posts-index-filtered');

    await page.locator('.posts-filter-note a').click();
    await expect(page.locator('.post-card')).toHaveCount(3);
  });

  test('paginates and preserves the category filter', async ({ page }) => {
    await page.goto('/blog');
    await expect(page.locator('.posts-pagination .page-info')).toHaveText('Page 1 of 2');

    await page.locator('.posts-pagination .next').click();
    await expect(page).toHaveURL(/\?page=1$/);
    const cards = page.locator('.post-card');
    await expect(cards).toHaveCount(1);
    await expect(cards.locator('h2')).toHaveText('First Trip');
    // Page 2 has no featured card
    await expect(cards).not.toHaveClass(/featured/);

    // Filtered views fit on one page, so no pagination is rendered
    await page.goto('/blog/category/gear');
    await expect(page.locator('.post-card')).toHaveCount(2);
    await expect(page.locator('.posts-pagination')).toHaveCount(0);
  });

  test('redirects legacy query-parameter category URLs', async ({ page }) => {
    await page.goto('/blog?category=gear');
    await expect(page).toHaveURL(/\/blog\/category\/gear$/);
    await expect(page.locator('.post-card')).toHaveCount(2);
  });

  test('serves RSS feeds globally and per category', async ({ page }) => {
    const feed = await page.request.get('/blog/feed.xml');
    expect(feed.status()).toBe(200);
    expect(feed.headers()['content-type']).toContain('application/rss+xml');
    const xml = await feed.text();
    expect(xml).toContain('<rss version="2.0"');
    expect(xml).toContain('<title>Plain Note</title>');
    expect(xml).toContain('<link>http://localhost:4319/blog/first-trip</link>');
    // Restricted posts are excluded from anonymous feeds
    expect(xml).not.toContain('Secret Note');

    const gearFeed = await page.request.get('/blog/category/gear/feed.xml');
    expect(gearFeed.status()).toBe(200);
    const gearXml = await gearFeed.text();
    expect(gearXml).toContain('<title>Camera Bag</title>');
    expect(gearXml).toContain('<title>Second Trip</title>');
    expect(gearXml).not.toContain('Plain Note');

    const missing = await page.request.get('/blog/category/nonexistent/feed.xml');
    expect(missing.status()).toBe(404);
  });

  test('shows an empty state for unknown categories', async ({ page }) => {
    await page.goto('/blog?category=nonexistent');
    await expect(page.locator('.post-card')).toHaveCount(0);
    await expect(page.locator('.posts-empty')).toContainText('No posts found');
  });
});

test.describe('post detail', () => {
  test('renders content, categories, and social meta tags', async ({ page }) => {
    await page.goto('/blog/first-trip');

    await expect(page.locator('.post-header h1')).toHaveText('First Trip');
    await expect(page.locator('.post-meta')).toContainText('January 1, 2024');
    await expect(page.locator('.post-meta')).toContainText('min read');

    // Category labels link back to the filtered index
    const label = page.locator('.post-categories .category-label');
    await expect(label).toHaveText('Travel');
    expect(await label.getAttribute('href')).toBe('/blog/category/travel');

    // Open Graph tags including og:image from the hero
    const ogType = page.locator('meta[property="og:type"]');
    await expect(ogType).toHaveAttribute('content', 'article');
    const ogImage = page.locator('meta[property="og:image"]');
    await expect(ogImage).toHaveAttribute('content', /localhost:4319\/g\/_image\//);
    const ogUrl = page.locator('meta[property="og:url"]');
    await expect(ogUrl).toHaveAttribute('content', 'http://localhost:4319/blog/first-trip');
  });

  test('omits og:image when the post has no images', async ({ page }) => {
    await page.goto('/blog/plain-note');
    await expect(page.locator('meta[property="og:image"]')).toHaveCount(0);
  });

  test('mounts the share bar with social links', async ({ page }) => {
    await page.goto('/blog/first-trip');

    const share = page.locator('.post-share');
    await expect(share).toBeVisible();

    const bluesky = share.locator('a', { hasText: 'Bluesky' });
    expect(await bluesky.getAttribute('href')).toContain('bsky.app/intent/compose');
    expect(await bluesky.getAttribute('href')).toContain(
      encodeURIComponent('http://localhost:4319/blog/first-trip'),
    );

    const email = share.locator('a', { hasText: 'Email' });
    expect(await email.getAttribute('href')).toContain('mailto:');

    await expect(share.locator('button', { hasText: 'Copy link' })).toBeVisible();
    await shot(page, 'post-detail-share');
  });

  test('copy link button copies the post URL', async ({ page, context }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    await page.goto('/blog/first-trip');

    await page.locator('.post-share button', { hasText: 'Copy link' }).click();
    await expect(page.locator('.post-share button', { hasText: 'Copied!' })).toBeVisible();

    const copied = await page.evaluate(() => navigator.clipboard.readText());
    expect(copied).toBe('http://localhost:4319/blog/first-trip');
  });

  test('mastodon button reveals an instance form', async ({ page, context }) => {
    // Keep the popup from hitting the real instance (it would redirect)
    await context.route('https://mastodon.social/**', (route) =>
      route.fulfill({ status: 200, body: '' }),
    );
    await page.goto('/blog/first-trip');

    await page.locator('.post-share button', { hasText: 'Mastodon' }).click();
    const input = page.locator('.post-share-mastodon-form input');
    await expect(input).toBeVisible();

    // Submitting opens the instance's share page in a new tab
    await input.fill('mastodon.social');
    const popupPromise = page.waitForEvent('popup');
    await page.locator('.post-share-mastodon-form button', { hasText: 'Go' }).click();
    const popup = await popupPromise;
    expect(popup.url()).toContain('mastodon.social/share');
    await popup.close();
  });
});
