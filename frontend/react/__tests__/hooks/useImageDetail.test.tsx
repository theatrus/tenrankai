import { describe, it, expect } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { server } from '../mocks/server';
import { useImageDetail } from '@hooks/useImageDetail';
import { createImageDetailData, createImageUserMetadata } from '../fixtures';

describe('useImageDetail', () => {
  it('initializes with null data and no loading', () => {
    const { result } = renderHook(() => useImageDetail({ galleryName: 'main' }));

    expect(result.current.data).toBeNull();
    expect(result.current.loading).toBe(false);
    expect(result.current.error).toBeNull();
  });

  it('initializes with provided initial data', () => {
    const initialData = createImageDetailData();
    const { result } = renderHook(() =>
      useImageDetail({ galleryName: 'main', initialData }),
    );

    expect(result.current.data).toEqual(initialData);
    expect(result.current.loading).toBe(false);
  });

  it('fetches image data via loadImage', async () => {
    const { result } = renderHook(() => useImageDetail({ galleryName: 'main' }));

    await act(async () => {
      await result.current.loadImage('test-image.jpg');
    });

    expect(result.current.data).not.toBeNull();
    expect(result.current.data!.gallery_name).toBe('main');
    expect(result.current.data!.image.path).toBe('test-image.jpg');
    expect(result.current.loading).toBe(false);
    expect(result.current.error).toBeNull();
  });

  it('sets error state on fetch failure', async () => {
    server.use(
      http.get('/api/gallery/:gallery/image/:path', () => {
        return new HttpResponse(null, { status: 500, statusText: 'Internal Server Error' });
      }),
    );

    const { result } = renderHook(() => useImageDetail({ galleryName: 'main' }));

    await act(async () => {
      await result.current.loadImage('broken.jpg');
    });

    expect(result.current.data).toBeNull();
    expect(result.current.error).toBeTruthy();
    expect(result.current.loading).toBe(false);
  });

  it('refetches current image data', async () => {
    const { result } = renderHook(() => useImageDetail({ galleryName: 'main' }));

    await act(async () => {
      await result.current.loadImage('test-image.jpg');
    });

    expect(result.current.data).not.toBeNull();

    const custom = createImageDetailData({
      image: {
        ...createImageDetailData().image,
        name: 'refetched-image.jpg',
      },
    });

    server.use(
      http.get('/api/gallery/:gallery/image/:path', () => {
        return HttpResponse.json(custom);
      }),
    );

    await act(async () => {
      await result.current.refetch();
    });

    expect(result.current.data!.image.name).toBe('refetched-image.jpg');
  });

  it('does nothing on refetch when no data is loaded', async () => {
    const { result } = renderHook(() => useImageDetail({ galleryName: 'main' }));

    await act(async () => {
      await result.current.refetch();
    });

    expect(result.current.data).toBeNull();
    expect(result.current.loading).toBe(false);
  });

  it('optimistically updates metadata', async () => {
    const { result } = renderHook(() => useImageDetail({ galleryName: 'main' }));

    await act(async () => {
      await result.current.loadImage('test-image.jpg');
    });

    const newMetadata = createImageUserMetadata({
      highlighted: true,
      tags: ['sunset', 'landscape'],
      comments: [],
    });

    act(() => {
      result.current.updateMetadata(newMetadata);
    });

    expect(result.current.data!.image.user_metadata).toEqual(newMetadata);
    expect(result.current.data!.image.user_metadata!.highlighted).toBe(true);
    expect(result.current.data!.image.user_metadata!.tags).toEqual(['sunset', 'landscape']);
  });

  it('does not update metadata when no data is loaded', () => {
    const { result } = renderHook(() => useImageDetail({ galleryName: 'main' }));

    const newMetadata = createImageUserMetadata({ highlighted: true });

    act(() => {
      result.current.updateMetadata(newMetadata);
    });

    expect(result.current.data).toBeNull();
  });
});
