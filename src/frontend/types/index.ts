// Common types for React components

export interface CameraInfo {
  camera_make?: string;
  camera_model?: string;
  lens_model?: string;
  iso?: number;
  aperture?: string;
  shutter_speed?: string;
  focal_length?: string;
  
  // Astronomical imaging fields
  telescope?: string;
  mount?: string;
  filters?: string;
  total_exposure_time?: number; // in hours
  ra?: string;  // Right Ascension
  dec?: string; // Declination
  
  // Additional technical details
  additional_details?: string;
}

export interface LocationInfo {
  latitude: number;
  longitude: number;
  google_maps_url: string;
  apple_maps_url: string;
}

export interface Comment {
  id: string;
  author: string;
  text: string;
  created_at: string;
  edited_at?: string;
  image_area?: ImageArea;
}

export interface ImageArea {
  x: number;       // Percentage (0-100)
  y: number;       // Percentage (0-100)
  width: number;   // Percentage (0-100)
  height: number;  // Percentage (0-100)
}

export type PickStatus = 'pick' | 'no_pick';

export interface ImageUserMetadata {
  comments: Comment[];
  highlighted: boolean;
  pick_status?: PickStatus;
  tags: string[];
  last_modified?: string;
  modified_by?: string;

  // AI-generated metadata
  ai_keywords?: string[];
  ai_alt_text?: string;
  ai_analyzed_at?: string;
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
  camera_info?: CameraInfo;
  location_info?: LocationInfo;
  color_profile?: string;
  is_new: boolean;
  user_metadata?: ImageUserMetadata;
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

export interface RolePermissions {
  can_view: boolean;
  can_see_technical_details: boolean;
  can_see_exact_dates: boolean;
  can_see_location: boolean;
  can_download_medium: boolean;
  can_download_large: boolean;
  can_download_original: boolean;
  can_download_gallery: boolean;
  can_read_metadata: boolean;
  can_add_comments: boolean;
  can_edit_own_comments: boolean;
  can_delete_own_comments: boolean;
  can_set_picks: boolean;
  can_add_tags: boolean;
  can_edit_any_comments: boolean;
  can_delete_any_comments: boolean;
  can_use_zoom: boolean;
  can_use_tile_zoom: boolean;
  can_analyze_images: boolean;
  can_see_ai_analysis: boolean;
  can_see_ai_alt_text: boolean;
  owner_access: boolean;
}

export interface TileConfig {
  tile_size: number;       // Size of each tile in pixels
  grid_width: number;      // Number of tiles horizontally
  grid_height: number;     // Number of tiles vertically  
  tiled_width: number;     // Actual width of the tiled area (capped at 8192)
  tiled_height: number;    // Actual height of the tiled area (capped at 8192)
}

export interface ImageDetailData {
  gallery_name: string;
  image: ImageInfo;
  breadcrumbs: BreadcrumbItem[];
  prev_image?: NavigationImage;
  next_image?: NavigationImage;
  /** Extended navigation: multiple previous images (closest first) */
  prev_images: NavigationImage[];
  /** Extended navigation: multiple next images (closest first) */
  next_images: NavigationImage[];
  permissions: RolePermissions;
  tile_config?: TileConfig;
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