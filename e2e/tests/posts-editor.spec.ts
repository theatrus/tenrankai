import { test, expect, Page } from '@playwright/test';
import { bootstrapSession } from './auth';
import { shot } from './helpers';

// Posts permission fixtures (e2e/fixtures/posts):
//   _folder.md          — public viewer role; e2euser gets can_edit_content
//   private/_folder.md  — no public or default access; e2euser gets owner role
//   private/secret-note — post only e2euser can see
const CREATED_SLUG = 'e2e-editor-post';

async function deleteIfExists(page: Page, slug: string) {
  await page.request.delete(`/api/posts/blog/source/${slug}`).catch(() => {});
}

test.describe('posts permissions (anonymous)', () => {
  test('hides private posts and edit controls', async ({ page }) => {
    await page.goto('/blog');

    // 4 public fixture posts over 2 pages; the private post is filtered out
    await expect(page.locator('.posts-pagination .page-info')).toHaveText('Page 1 of 2');
    await expect(page.locator('#post-new-mount')).toHaveCount(0);
    await expect(page.locator('.category-chip', { hasText: 'Private' })).toHaveCount(0);

    // Direct access to the private post 404s
    const response = await page.goto('/blog/private/secret-note');
    expect(response?.status()).toBe(404);
  });

  test('rejects write APIs', async ({ page }) => {
    const create = await page.request.post('/api/posts/blog/source', {
      data: { slug: 'nope', title: 'x', summary: 'y', content: '' },
    });
    expect(create.status()).toBe(403);

    const update = await page.request.put('/api/posts/blog/source/first-trip', {
      data: { title: 'x', summary: 'y', content: '' },
    });
    expect(update.status()).toBe(403);

    const del = await page.request.delete('/api/posts/blog/source/first-trip');
    expect(del.status()).toBe(403);

    const source = await page.request.get('/api/posts/blog/source/first-trip');
    expect(source.status()).toBe(403);
  });
});

test.describe('posts editor (authenticated)', () => {
  test.beforeEach(async ({ context }) => {
    await bootstrapSession(context);
  });

  test('sees private posts and edit controls', async ({ page }) => {
    await page.goto('/blog');
    await expect(page.locator('.post-new-btn')).toBeVisible();
    await expect(page.locator('.category-chip', { hasText: 'Private' })).toBeVisible();

    await page.goto('/blog/private/secret-note');
    await expect(page.locator('.post-header > h1')).toHaveText('Secret Note');
    await expect(page.locator('.post-edit-btn')).toBeVisible();

    // Authenticated feeds include restricted posts
    const feed = await page.request.get('/blog/feed.xml');
    expect(await feed.text()).toContain('<title>Secret Note</title>');
  });

  test('creates, edits, and deletes a post through the UI', async ({ page }) => {
    // Recover from any earlier aborted run
    await deleteIfExists(page, CREATED_SLUG);

    // --- Create ---
    await page.goto('/blog');
    await page.locator('.post-new-btn').click();

    const modal = page.locator('.post-editor-modal');
    await expect(modal).toBeVisible();

    await modal.locator('#post-title').fill('E2E Editor Post');
    // Slug is derived from the title; override it with a known value
    await expect(modal.locator('#post-slug')).toHaveValue('e2e-editor-post');
    await modal.locator('#post-summary').fill('Created by the Playwright editor test.');
    await modal.locator('#post-categories').fill('E2E, Testing');

    // Enter content via the markdown textarea for deterministic input
    await modal.getByRole('button', { name: 'Markdown', exact: true }).click();
    await modal.locator('.markdown-editor-textarea').fill('# Hello\n\nWritten by a robot.');
    await shot(page, 'post-editor-create');

    await modal.getByRole('button', { name: 'Create' }).click();
    await page.waitForURL(`/blog/${CREATED_SLUG}`);
    await expect(page.locator('.post-header > h1')).toHaveText('E2E Editor Post');
    await expect(page.locator('.post-content')).toContainText('Written by a robot.');
    await expect(page.locator('.post-categories .category-label')).toHaveCount(2);

    // --- Edit ---
    await page.locator('.post-edit-btn').click();
    await expect(modal).toBeVisible();
    await modal.locator('#post-title').fill('E2E Editor Post (edited)');
    await modal.getByRole('button', { name: 'Save' }).click();
    await expect(page.locator('.post-header > h1')).toHaveText('E2E Editor Post (edited)');

    // --- Delete (two-step confirm) ---
    await page.locator('.post-edit-btn').click();
    await expect(modal).toBeVisible();
    await modal.getByRole('button', { name: 'Delete', exact: true }).click();
    await modal.getByRole('button', { name: 'Really delete?' }).click();
    await page.waitForURL('/blog');
    await expect(page.locator('.post-card', { hasText: 'E2E Editor Post' })).toHaveCount(0);

    // The post really is gone server-side
    const gone = await page.request.get(`/api/posts/blog/source/${CREATED_SLUG}`);
    expect(gone.status()).toBe(404);
  });

  test('rejects invalid slugs from the API', async ({ page }) => {
    const bad = await page.request.post('/api/posts/blog/source', {
      data: { slug: '_hidden', title: 'x', summary: 'y', content: '' },
    });
    expect(bad.status()).toBe(400);

    const conflict = await page.request.post('/api/posts/blog/source', {
      data: { slug: 'first-trip', title: 'x', summary: 'y', content: '' },
    });
    expect(conflict.status()).toBe(409);
  });
});
