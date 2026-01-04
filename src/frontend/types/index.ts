// Common types for React components

export interface ImageMetadata {
  title?: string;
  description?: string;
  camera?: string;
  lens?: string;
  settings?: {
    aperture?: string;
    shutter?: string;
    iso?: string;
  };
  gps?: {
    latitude: number;
    longitude: number;
  };
  date?: string;
}

export interface GalleryImage {
  path: string;
  title?: string;
  thumbnail_url: string;
  gallery_url: string;
  medium_url: string;
  large_url: string;
  metadata?: ImageMetadata;
  dimensions?: {
    width: number;
    height: number;
  };
}

export interface Gallery {
  name: string;
  display_name: string;
  description?: string;
  images: GalleryImage[];
  total_count: number;
}

export interface ApiError {
  message: string;
  status: number;
  type: 'client' | 'server' | 'network';
}