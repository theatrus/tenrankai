import { describe, it, expect } from 'vitest';
import { http, HttpResponse } from 'msw';
import { server } from '../mocks/server';
import { ImageDetailApiClient } from '@api/image-detail';
import { createImageDetailData } from '../fixtures';

const client = new ImageDetailApiClient();

describe('ImageDetailApiClient', () => {
  describe('fetchImageDetail', () => {
    it('returns image detail data on success', async () => {
      const result = await client.fetchImageDetail('main', 'test-image.jpg');

      expect(result.gallery_name).toBe('main');
      expect(result.image.path).toBe('test-image.jpg');
      expect(result.permissions.can_view).toBe(true);
    });

    it('returns custom fixture data when handler is overridden', async () => {
      const custom = createImageDetailData({
        gallery_name: 'custom-gallery',
        image: {
          ...createImageDetailData().image,
          path: 'custom.jpg',
          name: 'custom.jpg',
        },
      });

      server.use(
        http.get('/api/gallery/:gallery/image/:path', () => {
          return HttpResponse.json(custom);
        }),
      );

      const result = await client.fetchImageDetail('custom-gallery', 'custom.jpg');
      expect(result.gallery_name).toBe('custom-gallery');
      expect(result.image.path).toBe('custom.jpg');
    });

    it('throws ApiError on 404', async () => {
      server.use(
        http.get('/api/gallery/:gallery/image/:path', () => {
          return new HttpResponse(null, { status: 404, statusText: 'Not Found' });
        }),
      );

      await expect(client.fetchImageDetail('main', 'missing.jpg')).rejects.toThrow('HTTP 404');
    });

    it('throws ApiError on 500', async () => {
      server.use(
        http.get('/api/gallery/:gallery/image/:path', () => {
          return new HttpResponse(null, { status: 500, statusText: 'Internal Server Error' });
        }),
      );

      await expect(client.fetchImageDetail('main', 'broken.jpg')).rejects.toThrow('HTTP 500');
    });

    it('thrown error has status and name properties', async () => {
      server.use(
        http.get('/api/gallery/:gallery/image/:path', () => {
          return new HttpResponse(null, { status: 404, statusText: 'Not Found' });
        }),
      );

      try {
        await client.fetchImageDetail('main', 'missing.jpg');
        expect.fail('Should have thrown');
      } catch (err) {
        expect(err).toBeInstanceOf(Error);
        expect((err as Error).name).toBe('ApiError');
        expect((err as any).status).toBe(404);
      }
    });

    it('handles URL-encoded gallery names and image paths', async () => {
      const result = await client.fetchImageDetail('my gallery', 'sub folder/image file.jpg');
      expect(result.gallery_name).toBe('main');
    });

    it('throws network error on fetch failure', async () => {
      server.use(
        http.get('/api/gallery/:gallery/image/:path', () => {
          return HttpResponse.error();
        }),
      );

      await expect(client.fetchImageDetail('main', 'image.jpg')).rejects.toThrow('Network error');
    });
  });

  describe('checkDownloadPermission', () => {
    it('returns true when authorized', async () => {
      const result = await client.checkDownloadPermission();
      expect(result).toBe(true);
    });

    it('returns false when not authorized', async () => {
      server.use(
        http.get('/api/verify', () => {
          return HttpResponse.json({ authorized: false });
        }),
      );

      const result = await client.checkDownloadPermission();
      expect(result).toBe(false);
    });

    it('returns false on HTTP error', async () => {
      server.use(
        http.get('/api/verify', () => {
          return new HttpResponse(null, { status: 401 });
        }),
      );

      const result = await client.checkDownloadPermission();
      expect(result).toBe(false);
    });

    it('returns false on network error', async () => {
      server.use(
        http.get('/api/verify', () => {
          return HttpResponse.error();
        }),
      );

      const result = await client.checkDownloadPermission();
      expect(result).toBe(false);
    });
  });
});
