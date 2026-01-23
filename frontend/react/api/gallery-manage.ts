export interface HideImagesResponse {
  success: boolean;
  hidden_images: string[];
}

export interface DeleteImagesResponse {
  success: boolean;
  deleted_count: number;
  errors?: string[];
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
};
