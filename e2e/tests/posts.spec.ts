import { test, expect } from '@playwright/test';
import { shot } from './helpers';

// Fixture posts (e2e/fixtures/posts), newest first:
//   Plain Note         2024-01-04  (no categories)
//   Camera Bag         2024-01-03  (Gear, hero from first content image)
//   Second Trip        2024-01-02  (Travel, Gear)
//   First Trip         2024-01-01  (Travel, explicit gallery hero)
//   Old Archived Note  2023-12-30  (Archive — declared archive=true in _categories.md,
//                                   hidden from the unfiltered index and main feed)
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
    await expect(chips).toHaveCount(4); // All, Gear, Travel, Archive
    await expect(chips.nth(0)).toHaveText('All');
    await expect(chips.nth(0)).toHaveClass(/active/);
    await expect(chips.nth(1)).toContainText('Gear');
    await expect(chips.nth(1).locator('.category-count')).toHaveText('2');
    await expect(chips.nth(2)).toContainText('Travel');
    await expect(chips.nth(2).locator('.category-count')).toHaveText('2');

    // The archive chip renders last, after a separator
    await expect(chips.nth(3)).toContainText('Archive');
    await expect(chips.nth(3)).toHaveClass(/archive/);
    await expect(page.locator('.category-bar-separator')).toHaveCount(1);
  });

  test('archive categories hide posts from the main flow', async ({ page }) => {
    // Archived posts are absent from both pages of the unfiltered index
    await page.goto('/blog');
    await expect(page.locator('.post-card', { hasText: 'Old Archived Note' })).toHaveCount(0);
    await expect(page.locator('.posts-pagination .page-info')).toHaveText('Page 1 of 2');
    await page.goto('/blog?page=1');
    await expect(page.locator('.post-card', { hasText: 'Old Archived Note' })).toHaveCount(0);

    // ...and from the main feed
    const feed = await page.request.get('/blog/feed.xml');
    expect(await feed.text()).not.toContain('Old Archived Note');

    // The archive category page is the archive view, with its declared description
    await page.goto('/blog');
    await page.locator('.category-bar .category-chip.archive').click();
    await expect(page).toHaveURL(/\/blog\/category\/archive$/);
    await expect(page.locator('.post-card h2')).toHaveText('Old Archived Note');
    await expect(page.locator('.posts-category-description')).toHaveText(
      'Older posts kept for reference',
    );
    await shot(page, 'posts-archive-category');

    // Archived posts stay reachable at their permalinks and in their feed
    const post = await page.request.get('/blog/old-archived');
    expect(post.status()).toBe(200);
    const archiveFeed = await page.request.get('/blog/category/archive/feed.xml');
    const xml = await archiveFeed.text();
    expect(xml).toContain('<title>Old Archived Note</title>');
    expect(xml).toContain('<description>Older posts kept for reference</description>');
  });

  test('shows a subscribe link at the bottom of the page', async ({ page }) => {
    await page.goto('/blog');
    const link = page.locator('.posts-feed-footer .posts-feed-link');
    await expect(link).toBeVisible();
    await expect(link).toContainText('Subscribe via RSS');
    expect(await link.getAttribute('href')).toBe('/blog/feed.xml');

    // On a category page the link targets the category feed
    await page.goto('/blog/category/gear');
    const categoryLink = page.locator('.posts-feed-footer .posts-feed-link');
    await expect(categoryLink).toContainText('Gear');
    expect(await categoryLink.getAttribute('href')).toBe('/blog/category/gear/feed.xml');
  });

  test('renders hero images from gallery references', async ({ page }) => {
    await page.goto('/blog');

    // Camera Bag's hero falls back to the first content image (a gallery ref)
    const cameraBag = page.locator('.post-card', { hasText: 'Camera Bag' });
    const heroImg = cameraBag.locator('.post-card-image img');
    await expect(heroImg).toBeVisible();
    expect(await heroImg.getAttribute('src')).toContain('/g/_image/');

    // A derived hero is not repeated above the post body on the detail page
    await page.goto('/blog/camera-bag');
    await expect(page.locator('.post-hero')).toHaveCount(0);

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

  test('returns 404 for unknown categories', async ({ page }) => {
    const response = await page.goto('/blog/category/nonexistent');
    expect(response?.status()).toBe(404);
  });

  test('page heading does not become a second sticky header', async ({ page }) => {
    await page.goto('/blog');
    // The global site-chrome header rule must not leak onto .posts-header
    const position = await page
      .locator('.posts-header')
      .evaluate((el) => getComputedStyle(el).position);
    expect(position).toBe('static');

    // After scrolling, only the site header occupies the top of the viewport
    await page.mouse.wheel(0, 600);
    await page.waitForTimeout(200);
    const topElement = await page.evaluate(() => {
      const el = document.elementsFromPoint(window.innerWidth / 2, 20)[0];
      return el?.closest('body > header') ? 'site-header' : (el?.className || el?.tagName);
    });
    expect(topElement).toBe('site-header');
    await shot(page, 'posts-index-scrolled');
  });

  test('whole card is clickable', async ({ page }) => {
    await page.goto('/blog');
    // Click the summary text, not the title link. The stretched link overlays
    // the card (which Playwright reports as interception), so force the click
    // and let the browser hit-test it onto the overlay.
    await page
      .locator('.post-card', { hasText: 'Camera Bag' })
      .locator('.post-card-summary')
      .click({ force: true });
    await expect(page).toHaveURL(/\/blog\/camera-bag$/);

    // Category labels inside a card still navigate to the category, not the post
    await page.goto('/blog');
    await page
      .locator('.post-card', { hasText: 'Camera Bag' })
      .locator('.category-label', { hasText: 'Gear' })
      .click();
    await expect(page).toHaveURL(/\/blog\/category\/gear$/);
  });
});

test.describe('posts preview embed', () => {
  test('home page embeds a recent-posts summary block', async ({ page }) => {
    await page.goto('/');

    const component = page.locator('.posts-preview-component');
    await expect(component).toBeVisible();
    await expect(component.locator('.posts-preview-heading')).toHaveText('Latest Posts');

    // Default count of three, newest first, archived posts excluded
    const items = component.locator('.posts-preview-item');
    await expect(items).toHaveCount(3);
    await expect(items.nth(0).locator('.posts-preview-title')).toHaveText('Plain Note');
    await expect(items.nth(1).locator('.posts-preview-title')).toHaveText('Camera Bag');
    await expect(items.nth(2).locator('.posts-preview-title')).toHaveText('Second Trip');
    await expect(component.locator('.posts-preview-item', { hasText: 'Old Archived Note' })).toHaveCount(0);

    // Items carry meta, summary, and category labels; hero thumbs render
    await expect(items.nth(0).locator('.posts-preview-meta')).toContainText('January 4, 2024');
    await expect(items.nth(0).locator('.posts-preview-meta')).toContainText('min read');
    await expect(items.nth(1).locator('.posts-preview-category')).toHaveText('Gear');
    await expect(items.nth(1).locator('.posts-preview-thumb img')).toBeVisible();
    await shot(page, 'posts-preview-embed');

    // The whole item links to the post
    await items.nth(0).click();
    await expect(page).toHaveURL(/\/blog\/plain-note$/);

    // The footer link is server-rendered and goes to the posts index
    await page.goto('/');
    await expect(component.locator('.btn-explore')).toHaveText('View All Posts →');
    await component.locator('.btn-explore').click();
    await expect(page).toHaveURL(/\/blog$/);
  });

  test('list variant renders super-compact title and date rows', async ({ page }) => {
    await page.goto('/about');

    const component = page.locator('.posts-preview-component.variant-list');
    await expect(component).toBeVisible();
    await expect(component.locator('.posts-preview-heading')).toHaveText('Recent Writing');

    // count: 5 shows all four visible posts (archived excluded); rows carry
    // only a title and a date — no summaries, thumbs, or category labels
    const items = component.locator('.posts-preview-item');
    await expect(items).toHaveCount(4);
    await expect(items.nth(0).locator('.posts-preview-title')).toHaveText('Plain Note');
    await expect(items.nth(0).locator('.posts-preview-meta')).toHaveText('January 4, 2024');
    await expect(component.locator('.posts-preview-summary')).toHaveCount(0);
    await expect(component.locator('.posts-preview-thumb')).toHaveCount(0);
    await expect(component.locator('.posts-preview-category')).toHaveCount(0);
    await expect(items.filter({ hasText: 'Old Archived Note' })).toHaveCount(0);
    await shot(page, 'posts-preview-list-variant');

    await items.nth(3).click();
    await expect(page).toHaveURL(/\/blog\/first-trip$/);
  });
});

test.describe('post detail', () => {
  test('renders content, categories, and social meta tags', async ({ page }) => {
    await page.goto('/blog/first-trip');

    await expect(page.locator('.post-header > h1')).toHaveText('First Trip');
    await expect(page.locator('.post-meta')).toContainText('January 1, 2024');
    await expect(page.locator('.post-meta')).toContainText('min read');

    // Category labels link back to the filtered index
    const label = page.locator('.post-categories .category-label');
    await expect(label).toHaveText('Travel');
    expect(await label.getAttribute('href')).toBe('/blog/category/travel');

    // The explicit hero image renders above the post body and links to its
    // gallery detail page
    const hero = page.locator('.post-hero img');
    await expect(hero).toBeVisible();
    expect(await hero.getAttribute('src')).toContain('/g/_image/');
    const heroLink = page.locator('.post-hero .post-hero-link');
    expect(await heroLink.getAttribute('href')).toBe('/g/detail/by-filename%2F01-alpha.png');

    // Open Graph tags including og:image from the hero
    const ogType = page.locator('meta[property="og:type"]');
    await expect(ogType).toHaveAttribute('content', 'article');
    const ogImage = page.locator('meta[property="og:image"]');
    await expect(ogImage).toHaveAttribute('content', /localhost:4319\/g\/_image\//);
    const ogUrl = page.locator('meta[property="og:url"]');
    await expect(ogUrl).toHaveAttribute('content', 'http://localhost:4319/blog/first-trip');

    // Article namespace metadata, including categories
    await expect(page.locator('meta[property="article:published_time"]')).toHaveAttribute(
      'content',
      /^2024-01-01T/,
    );
    await expect(page.locator('meta[property="article:modified_time"]')).toHaveCount(1);
    await expect(page.locator('meta[property="article:section"]')).toHaveAttribute(
      'content',
      'Travel',
    );
    await expect(page.locator('meta[property="article:tag"]')).toHaveAttribute(
      'content',
      'Travel',
    );
    await expect(page.locator('meta[property="article:author"]')).toHaveCount(1);

    // Multi-category posts emit one article:tag per category
    await page.goto('/blog/second-trip');
    await expect(page.locator('meta[property="article:tag"]')).toHaveCount(2);
    await expect(page.locator('meta[property="article:section"]')).toHaveAttribute(
      'content',
      'Travel',
    );
  });

  test('gallery embeds with the details option show a hover card', async ({ page }) => {
    await page.goto('/blog/camera-bag');

    // The embed links to the gallery and carries the details data attributes
    const embed = page.locator('.post-content .gallery-image-details');
    await expect(embed).toHaveCount(1);
    expect(await embed.getAttribute('href')).toBe('/g/detail/by-filename%2F02-bravo.png');

    // Hovering reveals the details card
    await expect(page.locator('.gallery-hover-card')).toHaveCount(0);
    await embed.locator('img').hover();
    const card = page.locator('.gallery-hover-card');
    await expect(card).toBeVisible();
    await expect(card.locator('.gallery-hover-title')).toContainText('bravo');
    await shot(page, 'post-gallery-hover-details');

    // Moving away hides it again
    await page.locator('.post-header > h1').hover();
    await expect(card).toHaveCount(0);
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
