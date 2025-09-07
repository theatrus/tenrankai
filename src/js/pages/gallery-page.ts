import { MasonryLayout } from '../components/gallery/masonry-layout.js';
import { GalleryPreview } from '../components/gallery/gallery-preview.js';
import { DomUtils } from '../core/dom-utils.js';
import type { GalleryPageConfig } from '../core/types.js';

export class GalleryPage {
  private config: GalleryPageConfig;
  private masonryLayout?: MasonryLayout;
  private galleryPreview?: GalleryPreview;

  constructor(config: GalleryPageConfig) {
    this.config = config;
    this.init();
  }

  private init(): void {
    // Initialize masonry layout for gallery grid
    if (this.config.container.classList.contains('gallery-grid')) {
      this.initGalleryGrid();
    }

    // Initialize gallery preview if it's a preview component
    if (this.config.container.classList.contains('gallery-preview')) {
      this.initGalleryPreview();
    }

    // Setup anchor link handling for smooth scrolling
    this.setupAnchorHandling();
  }

  private initGalleryGrid(): void {
    this.masonryLayout = new MasonryLayout(this.config.container, {
      gap: 20,
      breakpoints: { 768: 1, 1024: 2, 1440: 3 },
      minColumnWidth: 300,
      ...this.config.masonryConfig
    });

    // Handle window resize and orientation changes
    window.addEventListener('orientationchange', () => {
      setTimeout(() => this.masonryLayout?.refresh(), 500);
    });
  }

  private initGalleryPreview(): void {
    // Get configuration from data attributes
    const refreshInterval = parseInt(
      this.config.container.getAttribute('data-refresh-interval') || '30000'
    );
    const imageCount = parseInt(
      this.config.container.getAttribute('data-image-count') || '6'
    );

    this.galleryPreview = new GalleryPreview(
      this.config.container,
      this.config.galleryName,
      {
        refreshInterval,
        imageCount,
        masonryConfig: this.config.masonryConfig
      }
    );
  }

  private setupAnchorHandling(): void {
    // Handle anchor links for smooth scrolling to breadcrumb sections
    document.addEventListener('click', (event) => {
      const target = event.target as HTMLElement;
      const anchor = target.closest('a[href^="#"]') as HTMLAnchorElement;
      
      if (!anchor) return;

      const href = anchor.getAttribute('href');
      if (!href) return;

      const targetElement = document.querySelector(href);
      if (!targetElement) return;

      event.preventDefault();
      
      targetElement.scrollIntoView({
        behavior: 'smooth',
        block: 'start'
      });
    });
  }

  public refresh(): void {
    this.masonryLayout?.refresh();
    this.galleryPreview?.refresh();
  }

  public destroy(): void {
    this.masonryLayout?.destroy();
    this.galleryPreview?.destroy();
  }
}

// Auto-initialize gallery pages
document.addEventListener('DOMContentLoaded', () => {
  // Find gallery containers and initialize them
  const galleryContainers = document.querySelectorAll<HTMLElement>(
    '[data-gallery-name]'
  );

  galleryContainers.forEach(container => {
    const galleryName = container.getAttribute('data-gallery-name');
    if (!galleryName) return;

    // Parse masonry config from data attribute if present
    let masonryConfig = {};
    const configStr = container.getAttribute('data-masonry-config');
    if (configStr) {
      try {
        masonryConfig = JSON.parse(configStr);
      } catch (error) {
        console.warn('Invalid masonry config JSON:', configStr, error);
      }
    }

    new GalleryPage({
      galleryName,
      container,
      masonryConfig
    });
  });
});