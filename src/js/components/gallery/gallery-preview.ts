import { ApiClient } from '../../core/api-client.js';
import { MasonryLayout } from './masonry-layout.js';
import { DomUtils } from '../../core/dom-utils.js';
import type { GalleryItem, MasonryConfig } from '../../core/types.js';

export class GalleryPreview {
  private container: HTMLElement;
  private galleryName: string;
  private refreshInterval: number;
  private imageCount: number;
  private masonryLayout?: MasonryLayout;
  private apiInterval?: number;
  private isDestroyed = false;

  constructor(
    container: HTMLElement,
    galleryName: string,
    options: {
      refreshInterval?: number;
      imageCount?: number;
      masonryConfig?: Partial<MasonryConfig>;
    } = {}
  ) {
    this.container = container;
    this.galleryName = galleryName;
    this.refreshInterval = options.refreshInterval ?? 30000; // 30 seconds
    this.imageCount = options.imageCount ?? 6;

    this.setupMasonryLayout(options.masonryConfig);
    this.setupImageReplacement();
  }

  private setupMasonryLayout(config?: Partial<MasonryConfig>): void {
    this.masonryLayout = new MasonryLayout(this.container, {
      gap: 20,
      breakpoints: { 768: 1, 1024: 2 },
      minColumnWidth: 350,
      ...config
    });
  }

  private setupImageReplacement(): void {
    if (this.refreshInterval <= 0) return;

    this.apiInterval = window.setInterval(async () => {
      if (this.isDestroyed) return;
      
      try {
        const data = await ApiClient.getGalleryPreview(this.galleryName, this.imageCount);
        await this.updateImages(data.images);
      } catch (error) {
        console.error('Failed to refresh gallery preview:', error);
      }
    }, this.refreshInterval);
  }

  private async updateImages(newImages: GalleryItem[]): Promise<void> {
    if (this.isDestroyed || newImages.length === 0) return;

    // Get current images
    const currentImages = Array.from(this.container.querySelectorAll('.gallery-item'));
    
    // Randomly select images to replace
    const imagesToReplace = this.selectRandomImages(currentImages, Math.min(2, newImages.length));
    
    // Create fade-out promises
    const fadeOutPromises = imagesToReplace.map(async (imageElement, index) => {
      const newImage = newImages[index];
      if (!newImage) return;

      const htmlElement = imageElement as HTMLElement;

      // Start fade out
      htmlElement.style.opacity = '0';
      htmlElement.style.transition = 'opacity 0.5s ease-in-out';

      // Wait for fade out
      await new Promise(resolve => setTimeout(resolve, 500));

      // Update image content
      await this.updateImageElement(htmlElement, newImage);

      // Fade in
      htmlElement.style.opacity = '1';
    });

    await Promise.all(fadeOutPromises);
    
    // Refresh layout after all updates
    this.masonryLayout?.refresh();
  }

  private selectRandomImages(images: Element[], count: number): Element[] {
    const shuffled = [...images].sort(() => Math.random() - 0.5);
    return shuffled.slice(0, count);
  }

  private async updateImageElement(element: HTMLElement, newImage: GalleryItem): Promise<void> {
    // Update the image source
    const img = element.querySelector('img') as HTMLImageElement;
    const link = element.querySelector('a') as HTMLAnchorElement;
    
    if (img && link) {
      // Preload the new image
      const preloadImg = new Image();
      await new Promise<void>((resolve, reject) => {
        preloadImg.onload = () => resolve();
        preloadImg.onerror = () => reject(new Error('Failed to load image'));
        preloadImg.src = newImage.thumbnail_url;
      });

      // Update the image and link
      img.src = newImage.thumbnail_url;
      img.alt = newImage.name;
      link.href = newImage.gallery_url;
      
      // Update any data attributes
      element.setAttribute('data-path', newImage.path);
      
      // Update image name if displayed
      const nameElement = element.querySelector('.image-name');
      if (nameElement) {
        nameElement.textContent = newImage.name;
      }

      // Add "new" indicator if applicable
      const newIndicator = element.querySelector('.new-indicator');
      if (newImage.is_new && !newIndicator) {
        const indicator = DomUtils.createElement('div', 
          { class: 'new-indicator' }, 
          ['NEW']
        );
        element.appendChild(indicator);
      } else if (!newImage.is_new && newIndicator) {
        newIndicator.remove();
      }
    }
  }

  public destroy(): void {
    this.isDestroyed = true;
    if (this.apiInterval) {
      clearInterval(this.apiInterval);
    }
    this.masonryLayout?.destroy();
  }

  public refresh(): void {
    this.masonryLayout?.refresh();
  }

  public pause(): void {
    if (this.apiInterval) {
      clearInterval(this.apiInterval);
      this.apiInterval = undefined;
    }
  }

  public resume(): void {
    if (!this.apiInterval && this.refreshInterval > 0) {
      this.setupImageReplacement();
    }
  }
}