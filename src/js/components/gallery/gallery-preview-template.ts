// Gallery preview functionality specific to the _gallery_preview.html.liquid template
import type { GalleryImage } from '../../core/types.js';

export interface GalleryPreviewConfig {
  galleryName: string;
  galleryUrl?: string; // Base gallery URL for navigation
}

export class GalleryPreviewTemplate {
  private previewGrid: HTMLElement;
  private previewComponent: HTMLElement;
  private galleryName: string;
  private galleryUrl: string;
  private previewImages: GalleryImage[] = [];
  private allAvailableImages: GalleryImage[] = [];
  private imageReplacementInterval?: number;
  private resizeTimeout?: number;

  constructor(config: GalleryPreviewConfig) {
    const previewGrid = document.getElementById('gallery-preview-grid');
    const previewComponent = document.getElementById('gallery-preview-component');
    
    if (!previewGrid || !previewComponent) {
      throw new Error('Gallery preview elements not found');
    }
    
    this.previewGrid = previewGrid;
    this.previewComponent = previewComponent;
    this.galleryName = config.galleryName;
    this.galleryUrl = config.galleryUrl || '/gallery'; // Default gallery URL
    
    this.init();
  }

  private async init(): Promise<void> {
    try {
      await this.fetchInitialImages();
      this.layoutPreviewMasonry();
      this.setupEventListeners();
      this.startDynamicReplacement();
    } catch (error) {
      console.error('Failed to initialize gallery preview:', error);
      this.previewComponent.style.display = 'none';
    }
  }

  private async fetchInitialImages(): Promise<void> {
    const apiUrl = `/api/gallery/${this.galleryName}/preview`;
    const response = await fetch(apiUrl);
    
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
    
    // Get the actual container width from the DOM
    const containerRect = this.previewGrid.getBoundingClientRect();
    const containerWidth = containerRect.width || Math.min(viewportWidth, 1200);
    
    const isIOS = /iPad|iPhone|iPod/.test(navigator.userAgent);
    const gap = 24; // 1.5rem
    
    if (viewportWidth <= 768) {
      // Mobile: single column centered with balanced width
      // Use actual container width to account for any padding/margins
      const horizontalPadding = 32; // 1rem on each side
      const availableWidth = Math.min(viewportWidth - (horizontalPadding * 2), containerWidth);
      // Use 85% of available width for better balance
      return Math.floor(availableWidth);
    } else {
      // Desktop: two columns
      const desktopPadding = isIOS ? 32 : 40;
      return (containerWidth - desktopPadding - gap) / 2;
    }
  }

  private calculatePreviewDisplayDimensions(originalWidth: number, originalHeight: number, maxWidth: number): { width: number; height: number } {
    if (originalWidth <= maxWidth) {
      return { width: originalWidth, height: originalHeight };
    } else {
      const ratio = maxWidth / originalWidth;
      return { 
        width: maxWidth, 
        height: Math.round(originalHeight * ratio)
      };
    }
  }

  private createPreviewImageElement(image: GalleryImage, displayDimensions: { width: number; height: number }): HTMLElement {
    const link = document.createElement('a');
    link.href = `${this.galleryUrl}/${image.parent_path}#${image.path}`;
    link.className = 'preview-item image-preview-item' + (image.is_new ? ' is-new' : '');
    link.style.width = displayDimensions.width + 'px';
    link.style.display = 'inline-block';
    link.setAttribute('data-image-path', image.path);
    
    const imageDiv = document.createElement('div');
    imageDiv.className = 'preview-image';
    imageDiv.style.width = displayDimensions.width + 'px';
    imageDiv.style.height = displayDimensions.height + 'px';
    imageDiv.style.backgroundColor = 'transparent';
    
    const img = document.createElement('img');
    const baseUrl = image.gallery_url || image.thumbnail_url;
    img.src = baseUrl;
    
    // Add srcset for high-DPI displays
    if (baseUrl) {
      const url2x = baseUrl.replace('?size=gallery', '?size=gallery@2x')
                           .replace('?size=thumbnail', '?size=thumbnail@2x');
      img.srcset = `${baseUrl} 1x, ${url2x} 2x`;
    }
    
    img.alt = image.name;
    img.width = displayDimensions.width;
    img.height = displayDimensions.height;
    img.style.width = displayDimensions.width + 'px';
    img.style.height = displayDimensions.height + 'px';
    img.style.objectFit = 'cover';
    img.style.display = 'block';
    
    imageDiv.appendChild(img);
    link.appendChild(imageDiv);
    
    return link;
  }

  public layoutPreviewMasonry(): void {
    const columnWidth = this.calculatePreviewColumnWidth();
    const viewportWidth = window.innerWidth;
    const numColumns = viewportWidth <= 768 ? 1 : 2;
    
    // Clear existing content
    const columns = this.previewGrid.querySelectorAll('.masonry-column');
    columns.forEach(col => {
      (col as HTMLElement).innerHTML = '';
    });
    
    // Hide/show columns based on viewport - ensure they have proper flexbox properties
    const col0 = columns[0] as HTMLElement;
    col0.style.display = 'flex';
    col0.style.flexDirection = 'column';
    col0.style.flex = '1';
    col0.style.minWidth = '0';
    
    if (columns[1]) {
      const col1 = columns[1] as HTMLElement;
      if (numColumns > 1) {
        col1.style.display = 'flex';
        col1.style.flexDirection = 'column';
        col1.style.flex = '1';
        col1.style.minWidth = '0';
      } else {
        col1.style.display = 'none';
      }
    }
    
    // Track column heights
    const columnHeights = new Array(numColumns).fill(0);
    
    // Process each image
    this.previewImages.forEach(image => {
      // Use default dimensions if not available
      const width = image.dimensions ? image.dimensions[0] : 800;
      const height = image.dimensions ? image.dimensions[1] : 600;
      
      const displayDimensions = this.calculatePreviewDisplayDimensions(
        width, 
        height, 
        columnWidth
      );
      
      // Find shortest column
      const shortestColumnIndex = columnHeights.indexOf(Math.min(...columnHeights));
      
      // Create and append image element
      const imageElement = this.createPreviewImageElement(image, displayDimensions);
      columns[shortestColumnIndex].appendChild(imageElement);
      
      // Update column height
      columnHeights[shortestColumnIndex] += displayDimensions.height + 24; // gap
    });
  }

  private setupEventListeners(): void {
    const isIOS = /iPad|iPhone|iPod/.test(navigator.userAgent);
    
    const handlePreviewResize = () => {
      clearTimeout(this.resizeTimeout);
      // Use longer timeout for iOS due to viewport changes during scroll
      const timeout = isIOS ? 300 : 150;
      this.resizeTimeout = window.setTimeout(() => this.layoutPreviewMasonry(), timeout);
    };
    
    window.addEventListener('resize', handlePreviewResize);
    
    // iOS-specific: Handle orientation changes
    if (isIOS) {
      window.addEventListener('orientationchange', () => {
        setTimeout(() => this.layoutPreviewMasonry(), 500); // Delay for iOS orientation animation
      });
    }
    
    // Clean up interval when page is unloaded
    window.addEventListener('beforeunload', () => {
      if (this.imageReplacementInterval) {
        clearInterval(this.imageReplacementInterval);
      }
    });
  }

  private async fetchMoreImages(): Promise<GalleryImage[]> {
    try {
      const apiUrl = `/api/gallery/${this.galleryName}/preview?count=20`;
      const response = await fetch(apiUrl);
      
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
      // Need to fetch more images
      const moreImages = await this.fetchMoreImages();
      if (moreImages.length > 0) {
        // Add only truly new images
        const newImages = moreImages.filter(newImg => 
          !this.allAvailableImages.some(existingImg => existingImg.path === newImg.path)
        );
        this.allAvailableImages.push(...newImages);
      }
    }
    
    if (this.allAvailableImages.length <= this.previewImages.length) {
      return; // Not enough unique images for replacement
    }
    
    // Select images not currently displayed
    const availableForReplacement = this.allAvailableImages.filter(img => 
      !this.previewImages.some(currentImg => currentImg.path === img.path)
    );
    
    if (availableForReplacement.length === 0) return;
    
    // Pick a random replacement image
    const replacementImages = this.selectRandomImages(availableForReplacement, 1);
    const newImage = replacementImages[0];
    
    // Pick a random current image to replace
    const indexToReplace = Math.floor(Math.random() * this.previewImages.length);
    
    // Update the data
    this.previewImages[indexToReplace] = newImage;
    
    // Re-layout to show the new image
    this.layoutPreviewMasonry();
  }

  private startDynamicReplacement(): void {
    // Start replacement after 2 seconds, then every 10 seconds
    setTimeout(() => {
      this.imageReplacementInterval = window.setInterval(() => {
        this.replaceRandomImage().catch(error => {
          console.error('Error during image replacement:', error);
        });
      }, 10000); // Replace image every 10 seconds
    }, 2000);
  }

  public destroy(): void {
    if (this.imageReplacementInterval) {
      clearInterval(this.imageReplacementInterval);
    }
    
    if (this.resizeTimeout) {
      clearTimeout(this.resizeTimeout);
    }
  }
}