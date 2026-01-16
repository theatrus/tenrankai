// Gallery-specific masonry layout functionality
import type { GalleryImage } from '../core/types.js';

export interface GalleryMasonryConfig {
  galleryUrl: string;
  images: GalleryImage[];
}

export class GalleryMasonry {
  private galleryGrid: HTMLElement;
  private images: GalleryImage[];
  private galleryUrl: string;
  private resizeTimeout?: number;

  constructor(config: GalleryMasonryConfig) {
    const grid = document.getElementById('gallery-grid');
    if (!grid) {
      throw new Error('Gallery grid element not found');
    }
    
    this.galleryGrid = grid;
    this.images = config.images;
    this.galleryUrl = config.galleryUrl;
    
    this.init();
  }

  private init(): void {
    this.layoutMasonry();
    this.setupEventListeners();
    this.handleAnchorLinks();
  }

  private calculateColumnWidth(): number {
    const viewportWidth = window.innerWidth;
    const containerWidth = Math.min(viewportWidth, 1200);
    
    // iOS-specific viewport handling
    const isIOS = /iPad|iPhone|iPod/.test(navigator.userAgent);
    const gap = 24; // 1.5rem
    
    if (viewportWidth <= 768) {
      // Mobile: single column with minimal padding
      const mobilePadding = isIOS ? 16 : 20;
      return containerWidth - mobilePadding;
    } else {
      // Desktop: two columns
      const desktopPadding = isIOS ? 32 : 40;
      return (containerWidth - desktopPadding - gap) / 2;
    }
  }

  private calculateDisplayDimensions(originalWidth: number, originalHeight: number, maxWidth: number): { width: number; height: number } {
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

  private createImageElement(image: GalleryImage, displayDimensions: { width: number; height: number }): HTMLElement {
    const itemDiv = document.createElement('div');
    itemDiv.className = 'image-item' + (image.is_new ? ' is-new' : '');
    itemDiv.id = image.path;
    itemDiv.setAttribute('data-path', image.path);
    itemDiv.style.width = displayDimensions.width + 'px';
    itemDiv.style.height = displayDimensions.height + 'px';
    
    const link = document.createElement('a');
    link.href = `${this.galleryUrl}/detail/${image.path}`;
    link.className = 'image-link';
    
    const img = document.createElement('img');
    img.src = image.gallery_url;
    img.srcset = `${image.gallery_url} 1x, ${image.gallery_url}@2x 2x`;
    img.alt = image.name;
    img.width = displayDimensions.width;
    img.height = displayDimensions.height;
    img.style.width = '100%';
    img.style.height = '100%';
    img.style.objectFit = 'cover';
    
    link.appendChild(img);
    itemDiv.appendChild(link);
    
    return itemDiv;
  }

  public layoutMasonry(): void {
    const columnWidth = this.calculateColumnWidth();
    const viewportWidth = window.innerWidth;
    const numColumns = viewportWidth <= 768 ? 1 : 2;
    
    // Clear existing content
    const columns = this.galleryGrid.querySelectorAll('.masonry-column');
    columns.forEach(col => {
      (col as HTMLElement).innerHTML = '';
    });
    
    // Hide/show columns based on viewport
    (columns[0] as HTMLElement).style.display = 'flex';
    if (columns[1]) {
      (columns[1] as HTMLElement).style.display = numColumns > 1 ? 'flex' : 'none';
    }
    
    // Track column heights
    const columnHeights = new Array(numColumns).fill(0);
    
    // Process each image
    this.images.forEach(image => {
      // Use default dimensions if not available
      const width = image.dimensions ? image.dimensions[0] : 800;
      const height = image.dimensions ? image.dimensions[1] : 600;
      
      const displayDimensions = this.calculateDisplayDimensions(
        width, 
        height, 
        columnWidth
      );
      
      // Find shortest column
      const shortestColumnIndex = columnHeights.indexOf(Math.min(...columnHeights));
      
      // Create and append image element
      const imageElement = this.createImageElement(image, displayDimensions);
      columns[shortestColumnIndex].appendChild(imageElement);
      
      // Update column height
      columnHeights[shortestColumnIndex] += displayDimensions.height + 24; // gap
    });
  }

  private setupEventListeners(): void {
    const isIOS = /iPad|iPhone|iPod/.test(navigator.userAgent);
    
    const handleResize = () => {
      clearTimeout(this.resizeTimeout);
      // Use longer timeout for iOS due to viewport changes during scroll
      const timeout = isIOS ? 300 : 150;
      this.resizeTimeout = window.setTimeout(() => this.layoutMasonry(), timeout);
    };
    
    window.addEventListener('resize', handleResize);
    
    // iOS-specific: Handle orientation changes
    if (isIOS) {
      window.addEventListener('orientationchange', () => {
        setTimeout(() => this.layoutMasonry(), 500); // Delay for iOS orientation animation
      });
    }
  }

  private handleAnchorLinks(): void {
    // Handle anchor links after layout is complete
    if (window.location.hash) {
      setTimeout(() => {
        const targetElement = document.getElementById(window.location.hash.substring(1));
        if (targetElement) {
          targetElement.scrollIntoView({ behavior: 'smooth' });
        }
      }, 100);
    }
  }
}