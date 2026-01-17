// API client for content editing (folder/image descriptions)

export interface UpdateDescriptionRequest {
  description: string;
  title?: string;
}

export interface UpdateDescriptionResponse {
  success: boolean;
  description_html: string;
  description_markdown: string;
  title?: string;
}

export class ContentEditorApiClient {
  /**
   * Update folder description (_folder.md)
   */
  async updateFolderDescription(
    galleryName: string,
    folderPath: string,
    description: string,
    title?: string
  ): Promise<UpdateDescriptionResponse> {
    const encodedGallery = encodeURIComponent(galleryName);

    // Build URL - root folder uses different endpoint
    // Note: route is /folder-description/{path} because catch-all must be at end
    const url = folderPath
      ? `/api/gallery/${encodedGallery}/folder-description/${encodeURIComponent(folderPath)}`
      : `/api/gallery/${encodedGallery}/folder-description`;

    const body: UpdateDescriptionRequest = { description };
    if (title !== undefined) {
      body.title = title;
    }

    const response = await fetch(url, {
      method: 'PUT',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(body),
    });

    if (!response.ok) {
      const errorData = await response.json().catch(() => ({}));
      throw new ContentEditorError(
        errorData.message || `HTTP ${response.status}: ${response.statusText}`,
        response.status
      );
    }

    return await response.json();
  }

  /**
   * Update image description (.md sidecar file)
   */
  async updateImageDescription(
    galleryName: string,
    imagePath: string,
    description: string,
    title?: string
  ): Promise<UpdateDescriptionResponse> {
    const encodedGallery = encodeURIComponent(galleryName);
    const encodedPath = encodeURIComponent(imagePath);

    // Note: route is /image-description/{path} because catch-all must be at end
    const url = `/api/gallery/${encodedGallery}/image-description/${encodedPath}`;

    const body: UpdateDescriptionRequest = { description };
    if (title !== undefined) {
      body.title = title;
    }

    const response = await fetch(url, {
      method: 'PUT',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(body),
    });

    if (!response.ok) {
      const errorData = await response.json().catch(() => ({}));
      throw new ContentEditorError(
        errorData.message || `HTTP ${response.status}: ${response.statusText}`,
        response.status
      );
    }

    return await response.json();
  }
}

export class ContentEditorError extends Error {
  constructor(
    public override message: string,
    public status: number
  ) {
    super(message);
    this.name = 'ContentEditorError';
  }
}

// Singleton instance for convenience
export const contentEditorApi = new ContentEditorApiClient();
