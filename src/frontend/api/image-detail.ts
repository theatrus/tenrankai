import { ImageDetailData } from '../types/index.ts';

export class ImageDetailApiClient {
  constructor() {
    // No baseUrl needed for relative paths
  }

  async fetchImageDetail(galleryName: string, imagePath: string): Promise<ImageDetailData> {
    try {
      const url = `/api/gallery/${encodeURIComponent(galleryName)}/image/${encodeURIComponent(imagePath)}`;
      const response = await fetch(url);

      if (!response.ok) {
        throw new ApiError(`HTTP ${response.status}: ${response.statusText}`, response.status);
      }

      const data = await response.json();
      return data as ImageDetailData;
    } catch (error) {
      if (error instanceof ApiError) {
        throw error;
      }
      const message = error instanceof Error ? error.message : 'Unknown error';
      throw new ApiError(`Network error: ${message}`, 0);
    }
  }

  async checkDownloadPermission(): Promise<boolean> {
    try {
      const response = await fetch('/api/verify');
      if (!response.ok) {
        return false;
      }
      const data = await response.json();
      return data.authorized || false;
    } catch (error) {
      console.warn('Failed to check download permission:', error);
      return false;
    }
  }
}

// Custom error class
class ApiError extends Error {
  constructor(public message: string, public status: number) {
    super(message);
    this.name = 'ApiError';
  }
}