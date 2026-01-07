import { useState } from 'react';
import { ImageDetailData } from '../types/index.ts';
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
}

export function useImageDetail({ galleryName, initialData }: UseImageDetailOptions): UseImageDetailReturn {
  const [data, setData] = useState<ImageDetailData | null>(initialData || null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  const apiClient = new ImageDetailApiClient();

  const loadImage = async (imagePath: string) => {
    setLoading(true);
    setError(null);
    
    try {
      const imageData = await apiClient.fetchImageDetail(galleryName, imagePath);
      setData(imageData);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load image');
      console.error('Failed to load image detail:', err);
    } finally {
      setLoading(false);
    }
  };

  const refetch = async () => {
    if (data?.image.path) {
      await loadImage(data.image.path);
    }
  };

  return {
    data,
    loading,
    error,
    refetch,
    loadImage
  };
}