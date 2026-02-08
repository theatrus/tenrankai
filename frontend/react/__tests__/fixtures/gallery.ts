import type { GalleryData, GalleryItem } from '@/types/index';
import { createRolePermissions } from './permissions';
import { createBreadcrumbItem } from './images';

export function createGalleryItem(overrides?: Partial<GalleryItem>): GalleryItem {
  return {
    path: 'test-image.jpg',
    name: 'test-image.jpg',
    is_directory: false,
    is_new: false,
    ...overrides,
  };
}

export function createGalleryData(overrides?: Partial<GalleryData>): GalleryData {
  return {
    site_name: 'default',
    gallery_name: 'main',
    gallery_path: '/gallery/main',
    is_root: true,
    breadcrumbs: [
      createBreadcrumbItem({ name: 'main', display_name: 'Main Gallery', path: '/gallery/main', is_current: true }),
    ],
    directories: [],
    images: [],
    page: 1,
    total_pages: 1,
    permissions: createRolePermissions(),
    grid_mode: 'masonry',
    max_columns: 2,
    ...overrides,
  };
}

export { createBreadcrumbItem } from './images';
