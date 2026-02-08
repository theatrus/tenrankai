import { http, HttpResponse } from 'msw';
import { createImageDetailData } from '../fixtures/images';

export const handlers = [
  http.get('/api/gallery/:gallery/image/:path', () => {
    return HttpResponse.json(createImageDetailData());
  }),

  http.get('/api/verify', () => {
    return HttpResponse.json({ authorized: true });
  }),

  http.put('/api/gallery/:gallery/folder-description/:path*', () => {
    return HttpResponse.json({
      success: true,
      description_html: '<p>Updated description</p>',
      description_markdown: 'Updated description',
    });
  }),

  http.put('/api/gallery/:gallery/folder-description', () => {
    return HttpResponse.json({
      success: true,
      description_html: '<p>Updated description</p>',
      description_markdown: 'Updated description',
    });
  }),

  http.put('/api/gallery/:gallery/image-description/:path', () => {
    return HttpResponse.json({
      success: true,
      description_html: '<p>Updated image description</p>',
      description_markdown: 'Updated image description',
    });
  }),

  http.post('/_admin/api/galleries/:gallery/folders/:path/images/hide', () => {
    return HttpResponse.json({
      success: true,
      hidden_images: ['image1.jpg'],
    });
  }),

  http.delete('/_admin/api/galleries/:gallery/images', () => {
    return HttpResponse.json({
      success: true,
      deleted_count: 1,
    });
  }),

  http.post('/_admin/api/galleries/:gallery/folders/:path/images/move', () => {
    return HttpResponse.json({
      success: true,
      moved_count: 1,
      errors: [],
    });
  }),

  http.post('/_admin/api/galleries/:gallery/folders/:path/images/copy', () => {
    return HttpResponse.json({
      success: true,
      copied_count: 1,
      errors: [],
    });
  }),

  http.get('/_admin/api/sites/:site/galleries/:gallery/folders', () => {
    return HttpResponse.json({
      folders: [
        { path: 'folder1', name: 'Folder 1', has_custom_permissions: false, image_count: 5 },
        { path: 'folder2', name: 'Folder 2', has_custom_permissions: true, image_count: 10 },
      ],
    });
  }),

  http.post('/_admin/api/galleries/:gallery/folders/:path/create', () => {
    return HttpResponse.json({
      success: true,
      folder_path: 'new-folder',
    });
  }),

  http.delete('/_admin/api/galleries/:gallery/folders/:path', () => {
    return HttpResponse.json({
      success: true,
      message: 'Folder deleted',
    });
  }),
];
