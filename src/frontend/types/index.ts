// Common types for React components

export interface CameraInfo {
  camera_make?: string;
  camera_model?: string;
  lens_model?: string;
  iso?: number;
  aperture?: string;
  shutter_speed?: string;
  focal_length?: string;
}

export interface LocationInfo {
  latitude: number;
  longitude: number;
  google_maps_url: string;
  apple_maps_url: string;
}

export interface ImageInfo {
  path: string;
  name: string;
  title?: string;
  description?: string;
  dimensions: [number, number];
  capture_date?: string;
  file_size: number;
  thumbnail_url: string;
  gallery_url?: string;
  medium_url: string;
  large_url?: string;
  camera_info?: CameraInfo;
  location_info?: LocationInfo;
  color_profile?: string;
  is_new: boolean;
}

export interface NavigationImage {
  path: string;
  name: string;
  thumbnail_url: string;
}

export interface BreadcrumbItem {
  name: string;
  display_name: string;
  path: string;
  is_current: boolean;
}

export interface ImageDetailData {
  gallery_name: string;
  image: ImageInfo;
  breadcrumbs: BreadcrumbItem[];
  prev_image?: NavigationImage;
  next_image?: NavigationImage;
}

export interface GalleryItem {
  path: string;
  name: string;
  title?: string;
  thumbnail_url?: string;
  gallery_url?: string;
  medium_url?: string;
  large_url?: string;
  dimensions?: [number, number];
  capture_date?: string;
  is_directory: boolean;
  is_new: boolean;
  directory_preview?: string[];
}

export interface GalleryData {
  gallery_name: string;
  gallery_path: string;
  is_root: boolean;
  breadcrumbs: BreadcrumbItem[];
  directories: GalleryItem[];
  images: GalleryItem[];
  page: number;
  total_pages: number;
  folder_title?: string;
  folder_description?: string;
}

export interface ApiError {
  message: string;
  status: number;
  type: 'client' | 'server' | 'network';
}