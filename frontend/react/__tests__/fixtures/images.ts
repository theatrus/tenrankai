import type {
  CameraInfo,
  LocationInfo,
  Comment,
  ImageUserMetadata,
  ImageInfo,
  NavigationImage,
  BreadcrumbItem,
  ImageDetailData,
} from '@/types/index';
import { createRolePermissions } from './permissions';

export function createCameraInfo(overrides?: Partial<CameraInfo>): CameraInfo {
  return {
    camera_make: 'Canon',
    camera_model: 'EOS R5',
    lens_model: 'RF 24-70mm F2.8 L IS USM',
    iso: 400,
    aperture: 'f/2.8',
    shutter_speed: '1/250',
    focal_length: '50mm',
    ...overrides,
  };
}

export function createLocationInfo(overrides?: Partial<LocationInfo>): LocationInfo {
  return {
    latitude: 35.6762,
    longitude: 139.6503,
    google_maps_url: 'https://maps.google.com/?q=35.6762,139.6503',
    apple_maps_url: 'https://maps.apple.com/?ll=35.6762,139.6503',
    ...overrides,
  };
}

export function createComment(overrides?: Partial<Comment>): Comment {
  return {
    id: 'comment-1',
    author: 'testuser',
    text: 'Great photo!',
    created_at: '2024-01-15T10:30:00Z',
    ...overrides,
  };
}

export function createImageUserMetadata(overrides?: Partial<ImageUserMetadata>): ImageUserMetadata {
  return {
    comments: [],
    highlighted: false,
    tags: [],
    ...overrides,
  };
}

export function createImageInfo(overrides?: Partial<ImageInfo>): ImageInfo {
  return {
    path: 'test-image.jpg',
    name: 'test-image.jpg',
    dimensions: [4000, 3000],
    file_size: 5242880,
    thumbnail_url: '/gallery/main/thumb/test-image.jpg?size=thumbnail',
    medium_url: '/gallery/main/thumb/test-image.jpg?size=medium',
    is_new: false,
    is_primary: true,
    ...overrides,
  };
}

export function createNavigationImage(overrides?: Partial<NavigationImage>): NavigationImage {
  return {
    path: 'nav-image.jpg',
    name: 'nav-image.jpg',
    thumbnail_url: '/gallery/main/thumb/nav-image.jpg?size=thumbnail',
    ...overrides,
  };
}

export function createBreadcrumbItem(overrides?: Partial<BreadcrumbItem>): BreadcrumbItem {
  return {
    name: 'main',
    display_name: 'Main Gallery',
    path: '/gallery/main',
    is_current: false,
    ...overrides,
  };
}

export function createImageDetailData(overrides?: Partial<ImageDetailData>): ImageDetailData {
  return {
    gallery_name: 'main',
    image: createImageInfo(),
    breadcrumbs: [
      createBreadcrumbItem({ name: 'main', display_name: 'Main Gallery', path: '/gallery/main', is_current: true }),
    ],
    prev_images: [],
    next_images: [],
    permissions: createRolePermissions(),
    ...overrides,
  };
}
