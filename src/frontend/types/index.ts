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

export interface Comment {
  id: string;
  author: string;
  text: string;
  created_at: string;
  edited_at?: string;
}

export type PickStatus = 'pick' | 'no_pick';

export interface ImageUserMetadata {
  comments: Comment[];
  highlighted: boolean;
  pick_status?: PickStatus;
  tags: string[];
  last_modified?: string;
  modified_by?: string;
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
  can_read_metadata: boolean;
  can_add_comments: boolean;
  can_edit_own_comments: boolean;
  can_delete_own_comments: boolean;
  can_set_picks: boolean;
  can_add_tags: boolean;
  can_edit_any_comments: boolean;
  can_delete_any_comments: boolean;
  can_use_zoom: boolean;
  owner_access: boolean;
}

export interface ImageDetailData {
  gallery_name: string;
  image: ImageInfo;
  breadcrumbs: BreadcrumbItem[];
  prev_image?: NavigationImage;
  next_image?: NavigationImage;
  permissions: RolePermissions;
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