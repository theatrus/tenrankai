import { useState, useMemo, useCallback, useRef } from 'react';
import { ImageDetailData, ImageUserMetadata } from '../types/index.ts';
import { ImageDetailApiClient } from '../api/image-detail.ts';

interface UseImageDetailOptions {
  galleryName: string;
  initialData?: ImageDetailData;
}

interface UseImageDetailReturn {
  data: ImageDetailData | null;
  loading: boolean;
  error: string | null;
  refetch: () => Promise<void>;
  loadImage: (imagePath: string) => Promise<void>;
  updateMetadata: (metadata: ImageUserMetadata) => void;
}

export function useImageDetail({ galleryName, initialData }: UseImageDetailOptions): UseImageDetailReturn {
  const [data, setData] = useState<ImageDetailData | null>(initialData || null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  const apiClient = useMemo(() => new ImageDetailApiClient(), []);
  const requestId = useRef(0);

  const loadImage = useCallback(async (imagePath: string) => {
    const id = ++requestId.current;
    setLoading(true);
    setError(null);
    
    try {
      const imageData = await apiClient.fetchImageDetail(galleryName, imagePath);
      if (id === requestId.current) setData(imageData);
    } catch (err) {
      if (id === requestId.current) setError(err instanceof Error ? err.message : 'Failed to load image');
      console.error('Failed to load image detail:', err);
    } finally {
      if (id === requestId.current) setLoading(false);
    }
  }, [apiClient, galleryName]);

  const refetch = async () => {
    if (data?.image.path) {
      await loadImage(data.image.path);
    }
  };
  
  const updateMetadata = (metadata: ImageUserMetadata) => {
    if (data) {
      setData({
        ...data,
        image: {
          ...data.image,
          user_metadata: metadata
        }
      });
    }
  };

  return {
    data,
    loading,
    error,
    refetch,
    loadImage,
    updateMetadata
  };
}