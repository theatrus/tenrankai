// Core TypeScript interfaces for Tenrankai frontend

export interface GalleryItem {
  path: string;
  is_directory: boolean;
  name: string;
  thumbnail_url: string;
  gallery_url: string;
  dimensions?: {
    width: number;
    height: number;
  };
  is_new?: boolean;
}

export interface GalleryImage {
  path: string;
  name: string;
  thumbnail_url: string;
  gallery_url: string;
  dimensions?: [number, number]; // [width, height] format used by templates
  is_new?: boolean;
}

export interface PreviewResponse {
  images: GalleryItem[];
  total_count: number;
}

export interface AuthCredentials {
  id: string;
  rawId: string;
  response: AuthenticatorAttestationResponse;
  type: 'public-key';
}

export interface MasonryConfig {
  gap: number;
  breakpoints: Record<number, number>;
  minColumnWidth: number;
}

export interface ApiError {
  message: string;
  status: number;
  type: 'network' | 'server' | 'client';
}

export interface LoginCredentials {
  username: string;
  password?: string;
}

export interface LoginResponse {
  success: boolean;
  message: string;
  redirect?: string;
}

export interface ImageDimensions {
  width: number;
  height: number;
}

export interface GalleryPageConfig {
  galleryName: string;
  container: HTMLElement;
  masonryConfig?: Partial<MasonryConfig>;
}

export interface ToastOptions {
  type: 'success' | 'error' | 'info' | 'warning';
  duration?: number;
  persistent?: boolean;
}