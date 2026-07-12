import { describe, expect, it } from 'vitest';
import { buildTileUrl, imageSrcSet, withImageSize, withRetryFragment } from '../../utils/imageUrls.ts';

describe('image URL helpers', () => {
  it('rewrites path-based artifact sizes', () => {
    expect(withImageSize('/portfolio/_image/abc123/gallery', 'gallery@2x')).toBe(
      '/portfolio/_image/abc123/gallery@2x',
    );
  });

  it('rewrites legacy query size parameters', () => {
    expect(withImageSize('/gallery/image/folder/photo.jpg?size=gallery', 'gallery@2x')).toBe(
      '/gallery/image/folder/photo.jpg?size=gallery@2x',
    );
  });

  it('builds path-based tile URLs from any gallery prefix', () => {
    expect(buildTileUrl('/portfolio/_image/folder%2Fphoto.jpg/medium', 4, 2)).toBe(
      '/portfolio/_image/folder%2Fphoto.jpg/tile_4_2',
    );
    expect(buildTileUrl('/portfolio/_image/folder%2Fphoto.jpg/medium', 4, 2, true)).toBe(
      '/portfolio/_image/folder%2Fphoto.jpg/tile_4_2@2x',
    );
  });

  it('builds legacy query-based tile URLs without changing the route', () => {
    expect(buildTileUrl('/gallery/image/folder/photo.jpg?size=medium', 4, 2)).toBe(
      '/gallery/image/folder/photo.jpg?size=tile_4_2',
    );
    expect(buildTileUrl('/gallery/image/folder/photo.jpg?size=medium', 4, 2, true)).toBe(
      '/gallery/image/folder/photo.jpg?size=tile_4_2@2x',
    );
  });

  it('builds srcset and retry fragments without changing the request path', () => {
    expect(imageSrcSet('/gallery/_image/abc/gallery', 'gallery@2x')).toBe(
      '/gallery/_image/abc/gallery 1x, /gallery/_image/abc/gallery@2x 2x',
    );
    expect(withRetryFragment('/gallery/_image/abc/gallery', 3)).toBe(
      '/gallery/_image/abc/gallery#retry-3',
    );
  });
});
