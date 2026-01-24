export interface HideImagesResponse {
  success: boolean;
  hidden_images: string[];
}

export interface DeleteImagesResponse {
  success: boolean;
  deleted_count: number;
  errors?: string[];
}

export interface MoveImagesResponse {
  success: boolean;
  moved_count: number;
  errors: string[];
}

export interface CopyImagesResponse {
  success: boolean;
  copied_count: number;
  errors: string[];
}

export interface FolderInfo {
  path: string;
  name: string;
  has_custom_permissions: boolean;
  image_count: number;
}

export interface CreateFolderRequest {
  name: string;
  description?: string;
}

export interface CreateFolderResponse {
  success: boolean;
  folder_path: string;
}

export interface DeleteFolderResponse {
  success: boolean;
  message: string;
}

class ApiError extends Error {
  constructor(public message: string, public status: number) {
    super(message);
    this.name = 'ApiError';
  }
}

async function adminRequest<T>(
  method: string,
  path: string,
  body?: unknown
): Promise<T> {
  const response = await fetch(`/_admin/api${path}`, {
    method,
    headers: { 'Content-Type': 'application/json' },
    credentials: 'same-origin',
    body: body ? JSON.stringify(body) : undefined,
  });

  if (!response.ok) {
    if (response.status === 401) {
      window.location.href = `/_login?return=${encodeURIComponent(window.location.pathname)}`;
      throw new ApiError('Unauthorized', 401);
    }
    const text = await response.text();
    throw new ApiError(text || `HTTP ${response.status}`, response.status);
  }

  return response.json();
}

export const galleryManageApi = {
  hideImages: (galleryName: string, folderPath: string, paths: string[], hide: boolean) =>
    adminRequest<HideImagesResponse>(
      'POST',
      `/galleries/${encodeURIComponent(galleryName)}/folders/${encodeURIComponent(folderPath || '_root')}/images/hide`,
      { paths, hide }
    ),

  deleteImages: (galleryName: string, paths: string[]) =>
    adminRequest<DeleteImagesResponse>(
      'DELETE',
      `/galleries/${encodeURIComponent(galleryName)}/images`,
      { paths }
    ),

  moveImages: (galleryName: string, folderPath: string, paths: string[], targetFolder: string) =>
    adminRequest<MoveImagesResponse>(
      'POST',
      `/galleries/${encodeURIComponent(galleryName)}/folders/${encodeURIComponent(folderPath || '_root')}/images/move`,
      { paths, target_folder: targetFolder }
    ),

  copyImages: (galleryName: string, folderPath: string, paths: string[], targetFolder: string) =>
    adminRequest<CopyImagesResponse>(
      'POST',
      `/galleries/${encodeURIComponent(galleryName)}/folders/${encodeURIComponent(folderPath || '_root')}/images/copy`,
      { paths, target_folder: targetFolder }
    ),

  listFolders: (site: string, galleryName: string) =>
    adminRequest<{ folders: FolderInfo[] }>(
      'GET',
      `/sites/${encodeURIComponent(site)}/galleries/${encodeURIComponent(galleryName)}/folders`
    ),

  createFolder: (galleryName: string, parentFolder: string, request: CreateFolderRequest) =>
    adminRequest<CreateFolderResponse>(
      'POST',
      `/galleries/${encodeURIComponent(galleryName)}/folders/${encodeURIComponent(parentFolder || '_root')}/create`,
      request
    ),

  deleteFolder: (galleryName: string, folderPath: string) =>
    adminRequest<DeleteFolderResponse>(
      'DELETE',
      `/galleries/${encodeURIComponent(galleryName)}/folders/${encodeURIComponent(folderPath || '_root')}`
    ),
};
