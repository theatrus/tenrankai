import { describe, it, expect } from 'vitest';
import { http, HttpResponse } from 'msw';
import { server } from '../mocks/server';
import { ContentEditorApiClient, ContentEditorError } from '@api/content-editor';

const client = new ContentEditorApiClient();

describe('ContentEditorApiClient', () => {
  describe('updateFolderDescription', () => {
    it('updates root folder description', async () => {
      const result = await client.updateFolderDescription('main', '', 'New description');
      expect(result.success).toBe(true);
      expect(result.description_html).toBe('<p>Updated description</p>');
      expect(result.description_markdown).toBe('Updated description');
    });

    it('updates subfolder description', async () => {
      const result = await client.updateFolderDescription('main', 'subfolder', 'New description');
      expect(result.success).toBe(true);
    });

    it('includes title when provided', async () => {
      const result = await client.updateFolderDescription('main', '', 'Desc', 'My Title');
      expect(result.success).toBe(true);
    });

    it('throws ContentEditorError on HTTP error', async () => {
      server.use(
        http.put('/api/gallery/:gallery/folder-description', () => {
          return HttpResponse.json({ message: 'Forbidden' }, { status: 403 });
        }),
      );

      await expect(client.updateFolderDescription('main', '', 'Desc')).rejects.toThrow('Forbidden');
    });

    it('throws with status text on non-JSON error', async () => {
      server.use(
        http.put('/api/gallery/:gallery/folder-description', () => {
          return new HttpResponse('Not allowed', {
            status: 403,
            statusText: 'Forbidden',
            headers: { 'Content-Type': 'text/plain' },
          });
        }),
      );

      try {
        await client.updateFolderDescription('main', '', 'Desc');
        expect.fail('Should have thrown');
      } catch (err) {
        expect(err).toBeInstanceOf(ContentEditorError);
        expect((err as ContentEditorError).status).toBe(403);
      }
    });
  });

  describe('updateImageDescription', () => {
    it('updates image description successfully', async () => {
      const result = await client.updateImageDescription('main', 'photo.jpg', 'Beautiful sunset');
      expect(result.success).toBe(true);
      expect(result.description_html).toBe('<p>Updated image description</p>');
    });

    it('includes title when provided', async () => {
      const result = await client.updateImageDescription('main', 'photo.jpg', 'Desc', 'Sunset');
      expect(result.success).toBe(true);
    });

    it('throws ContentEditorError on HTTP error', async () => {
      server.use(
        http.put('/api/gallery/:gallery/image-description/:path', () => {
          return HttpResponse.json({ message: 'Not Found' }, { status: 404 });
        }),
      );

      await expect(client.updateImageDescription('main', 'missing.jpg', 'Desc')).rejects.toThrow('Not Found');
    });
  });
});
