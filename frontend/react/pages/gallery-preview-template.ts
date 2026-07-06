interface GalleryImage {
  path: string;
  name: string;
  parent_path: string;
  thumbnail_url: string;
  gallery_url: string;
  dimensions?: [number, number];
  is_new?: boolean;
}

class GalleryPreviewTemplate {
  private previewGrid: HTMLElement;
  private previewComponent: HTMLElement;
  private galleryName: string;
  private galleryUrl: string;
  private previewImages: GalleryImage[] = [];
  private allAvailableImages: GalleryImage[] = [];
  private imageReplacementInterval?: number;
  private resizeTimeout?: number;

  constructor(previewComponent: HTMLElement) {
    const previewGrid = previewComponent.querySelector<HTMLElement>('#gallery-preview-grid');
    const galleryName = previewComponent.dataset.galleryName;

    if (!previewGrid || !galleryName) {
      throw new Error('Gallery preview configuration is incomplete');
    }

    this.previewGrid = previewGrid;
    this.previewComponent = previewComponent;
    this.galleryName = galleryName;
    this.galleryUrl = previewComponent.dataset.galleryUrl || '/gallery';

    void this.init();
  }

  private async init(): Promise<void> {
    try {
      await this.fetchInitialImages();
      requestAnimationFrame(() => {
        this.layoutPreviewMasonry();
        this.setupEventListeners();
        this.startDynamicReplacement();
      });
    } catch (error) {
      console.error('Failed to initialize gallery preview:', error);
      this.previewComponent.style.display = 'none';
    }
  }

  private async fetchInitialImages(): Promise<void> {
    const response = await fetch(`/api/gallery/${this.galleryName}/preview`);

    if (!response.ok) {
      throw new Error(`Failed to fetch gallery preview: ${response.status}`);
    }

    const data = await response.json();
    this.previewImages = data.images || [];
    this.allAvailableImages = [...this.previewImages];

    if (this.previewImages.length === 0) {
      throw new Error('No preview images available');
    }
  }

  private calculatePreviewColumnWidth(): number {
    const viewportWidth = window.innerWidth;
    const containerRect = this.previewGrid.getBoundingClientRect();
    const containerWidth = containerRect.width || Math.min(viewportWidth, 1200);
    const isIOS = /iPad|iPhone|iPod/.test(navigator.userAgent);
    const gap = 24;

    if (viewportWidth <= 768) {
      const horizontalPadding = 32;
      return Math.floor(Math.min(viewportWidth - horizontalPadding * 2, containerWidth));
    }

    const desktopPadding = isIOS ? 32 : 40;
    return (containerWidth - desktopPadding - gap) / 2;
  }

  private calculatePreviewDisplayDimensions(
    originalWidth: number,
    originalHeight: number,
    maxWidth: number,
  ): { width: number; height: number } {
    if (originalWidth <= maxWidth) {
      return { width: originalWidth, height: originalHeight };
    }

    const ratio = maxWidth / originalWidth;
    return {
      width: maxWidth,
      height: Math.round(originalHeight * ratio),
    };
  }

  private createPreviewImageElement(
    image: GalleryImage,
    displayDimensions: { width: number; height: number },
  ): HTMLElement {
    const link = document.createElement('a');
    link.href = `${this.galleryUrl}/${image.parent_path}#${image.path}`;
    link.className = `preview-item image-preview-item${image.is_new ? ' is-new' : ''}`;
    link.style.width = `${displayDimensions.width}px`;
    link.style.display = 'inline-block';
    link.dataset.imagePath = image.path;

    const imageDiv = document.createElement('div');
    imageDiv.className = 'preview-image';
    imageDiv.style.width = `${displayDimensions.width}px`;
    imageDiv.style.height = `${displayDimensions.height}px`;
    imageDiv.style.backgroundColor = 'transparent';

    const img = document.createElement('img');
    const baseUrl = image.gallery_url || image.thumbnail_url;
    img.src = baseUrl;

    if (baseUrl) {
      const url2x = baseUrl.replace('?size=gallery', '?size=gallery@2x').replace('?size=thumbnail', '?size=thumbnail@2x');
      img.srcset = `${baseUrl} 1x, ${url2x} 2x`;
    }

    img.alt = image.name;
    img.width = displayDimensions.width;
    img.height = displayDimensions.height;
    img.style.width = `${displayDimensions.width}px`;
    img.style.height = `${displayDimensions.height}px`;
    img.style.objectFit = 'cover';
    img.style.display = 'block';

    imageDiv.appendChild(img);
    link.appendChild(imageDiv);

    return link;
  }

  public layoutPreviewMasonry(): void {
    const columnWidth = this.calculatePreviewColumnWidth();
    const numColumns = window.innerWidth <= 768 ? 1 : 2;
    const columns = Array.from(this.previewGrid.querySelectorAll<HTMLElement>('.masonry-column'));

    columns.forEach((column) => {
      column.innerHTML = '';
    });

    const firstColumn = columns[0];
    if (!firstColumn) {
      return;
    }

    firstColumn.style.display = 'flex';
    firstColumn.style.flexDirection = 'column';
    firstColumn.style.flex = '1';
    firstColumn.style.minWidth = '0';

    if (columns[1]) {
      const secondColumn = columns[1];
      if (numColumns > 1) {
        secondColumn.style.display = 'flex';
        secondColumn.style.flexDirection = 'column';
        secondColumn.style.flex = '1';
        secondColumn.style.minWidth = '0';
      } else {
        secondColumn.style.display = 'none';
      }
    }

    const columnHeights = new Array(numColumns).fill(0);

    this.previewImages.forEach((image) => {
      const width = image.dimensions ? image.dimensions[0] : 800;
      const height = image.dimensions ? image.dimensions[1] : 600;
      const displayDimensions = this.calculatePreviewDisplayDimensions(width, height, columnWidth);
      const shortestColumnIndex = columnHeights.indexOf(Math.min(...columnHeights));
      const imageElement = this.createPreviewImageElement(image, displayDimensions);

      columns[shortestColumnIndex]?.appendChild(imageElement);
      columnHeights[shortestColumnIndex] += displayDimensions.height + 24;
    });
  }

  private setupEventListeners(): void {
    const isIOS = /iPad|iPhone|iPod/.test(navigator.userAgent);

    window.addEventListener('resize', () => {
      clearTimeout(this.resizeTimeout);
      const timeout = isIOS ? 300 : 150;
      this.resizeTimeout = window.setTimeout(() => this.layoutPreviewMasonry(), timeout);
    });

    if (isIOS) {
      window.addEventListener('orientationchange', () => {
        setTimeout(() => this.layoutPreviewMasonry(), 500);
      });
    }

    window.addEventListener('beforeunload', () => {
      if (this.imageReplacementInterval) {
        clearInterval(this.imageReplacementInterval);
      }
    });
  }

  private async fetchMoreImages(): Promise<GalleryImage[]> {
    try {
      const response = await fetch(`/api/gallery/${this.galleryName}/preview?count=20`);

      if (!response.ok) {
        console.error('Failed to fetch more gallery images:', response.status);
        return [];
      }

      const data = await response.json();
      return data.images || [];
    } catch (error) {
      console.error('Error fetching more images:', error);
      return [];
    }
  }

  private selectRandomImages(availableImages: GalleryImage[], count: number): GalleryImage[] {
    const shuffled = [...availableImages].sort(() => Math.random() - 0.5);
    return shuffled.slice(0, count);
  }

  private async replaceRandomImage(): Promise<void> {
    if (this.allAvailableImages.length <= this.previewImages.length) {
      const moreImages = await this.fetchMoreImages();
      if (moreImages.length > 0) {
        const newImages = moreImages.filter((newImage) => (
          !this.allAvailableImages.some((existingImage) => existingImage.path === newImage.path)
        ));
        this.allAvailableImages.push(...newImages);
      }
    }

    if (this.allAvailableImages.length <= this.previewImages.length) {
      return;
    }

    const availableForReplacement = this.allAvailableImages.filter((image) => (
      !this.previewImages.some((currentImage) => currentImage.path === image.path)
    ));

    if (availableForReplacement.length === 0) {
      return;
    }

    const [newImage] = this.selectRandomImages(availableForReplacement, 1);
    if (!newImage) {
      return;
    }

    const indexToReplace = Math.floor(Math.random() * this.previewImages.length);
    this.previewImages[indexToReplace] = newImage;
    this.layoutPreviewMasonry();
  }

  private startDynamicReplacement(): void {
    setTimeout(() => {
      this.imageReplacementInterval = window.setInterval(() => {
        void this.replaceRandomImage();
      }, 10000);
    }, 2000);
  }
}

document.addEventListener('DOMContentLoaded', () => {
  document.querySelectorAll<HTMLElement>('.gallery-preview-component').forEach((previewComponent) => {
    new GalleryPreviewTemplate(previewComponent);
  });
});
