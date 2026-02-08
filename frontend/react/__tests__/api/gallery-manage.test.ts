import { describe, it, expect } from 'vitest';
import { http, HttpResponse } from 'msw';
import { server } from '../mocks/server';
import { galleryManageApi } from '@api/gallery-manage';

describe('galleryManageApi', () => {
  describe('hideImages', () => {
    it('hides images successfully', async () => {
      const result = await galleryManageApi.hideImages('main', 'folder1', ['img1.jpg'], true);
      expect(result.success).toBe(true);
      expect(result.hidden_images).toEqual(['image1.jpg']);
    });

    it('uses _root for empty folder path', async () => {
      const result = await galleryManageApi.hideImages('main', '', ['img1.jpg'], true);
      expect(result.success).toBe(true);
    });
  });

  describe('deleteImages', () => {
    it('deletes images successfully', async () => {
      const result = await galleryManageApi.deleteImages('main', ['img1.jpg']);
      expect(result.success).toBe(true);
      expect(result.deleted_count).toBe(1);
    });

    it('throws on server error', async () => {
      server.use(
        http.delete('/_admin/api/galleries/:gallery/images', () => {
          return new HttpResponse('Server error', { status: 500 });
        }),
      );

      await expect(galleryManageApi.deleteImages('main', ['img.jpg'])).rejects.toThrow();
    });
  });

  describe('moveImages', () => {
    it('moves images successfully', async () => {
      const result = await galleryManageApi.moveImages('main', 'folder1', ['img1.jpg'], 'folder2');
      expect(result.success).toBe(true);
      expect(result.moved_count).toBe(1);
      expect(result.errors).toEqual([]);
    });
  });

  describe('copyImages', () => {
    it('copies images successfully', async () => {
      const result = await galleryManageApi.copyImages('main', 'folder1', ['img1.jpg'], 'folder2');
      expect(result.success).toBe(true);
      expect(result.copied_count).toBe(1);
      expect(result.errors).toEqual([]);
    });
  });

  describe('listFolders', () => {
    it('lists folders successfully', async () => {
      const result = await galleryManageApi.listFolders('default', 'main');
      expect(result.folders).toHaveLength(2);
      expect(result.folders[0].name).toBe('Folder 1');
      expect(result.folders[1].has_custom_permissions).toBe(true);
    });
  });

  describe('createFolder', () => {
    it('creates folder successfully', async () => {
      const result = await galleryManageApi.createFolder('main', 'parent', { name: 'new-folder' });
      expect(result.success).toBe(true);
      expect(result.folder_path).toBe('new-folder');
    });
  });

  describe('deleteFolder', () => {
    it('deletes folder successfully', async () => {
      const result = await galleryManageApi.deleteFolder('main', 'old-folder');
      expect(result.success).toBe(true);
      expect(result.message).toBe('Folder deleted');
    });
  });

  describe('401 redirect', () => {
    it('throws on 401 response', async () => {
      server.use(
        http.delete('/_admin/api/galleries/:gallery/images', () => {
          return new HttpResponse('Unauthorized', { status: 401 });
        }),
      );

      await expect(galleryManageApi.deleteImages('main', ['img.jpg'])).rejects.toThrow();
    });
  });
});
