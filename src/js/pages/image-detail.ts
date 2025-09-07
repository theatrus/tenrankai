// Image detail page functionality

export interface ImageDetailConfig {
  prevImagePath?: string;
  nextImagePath?: string;
  galleryUrl: string;
  imagePath: string;
  imageName: string;
}

export class ImageDetail {
  private config: ImageDetailConfig;
  private mainImage: HTMLImageElement;
  private imageContainer: HTMLElement;

  constructor(config: ImageDetailConfig) {
    this.config = config;
    
    const mainImage = document.getElementById('main-image') as HTMLImageElement;
    if (!mainImage) {
      throw new Error('Main image element not found');
    }
    
    this.mainImage = mainImage;
    this.imageContainer = mainImage.closest('.image-container') as HTMLElement;
    
    if (!this.imageContainer) {
      throw new Error('Image container not found');
    }

    this.init();
  }

  private init(): void {
    this.setupImageLoading();
    this.setupControls();
    this.setupKeyboardNavigation();
    this.preloadAdjacentImages();
  }

  private setupImageLoading(): void {
    // Add loading class initially
    this.imageContainer.classList.add('loading');
    
    // Remove loading class when image is loaded
    if (this.mainImage.complete && this.mainImage.naturalHeight !== 0) {
      this.imageContainer.classList.remove('loading');
    } else {
      this.mainImage.addEventListener('load', () => {
        this.imageContainer.classList.remove('loading');
      });
      
      this.mainImage.addEventListener('error', () => {
        this.imageContainer.classList.remove('loading');
      });
    }
  }

  private async checkDownloadPermission(): Promise<boolean> {
    try {
      const response = await fetch('/api/verify');
      const data = await response.json();
      return data.authorized;
    } catch (error) {
      console.error('Error checking download permission:', error);
      return false;
    }
  }

  private async setupControls(): Promise<void> {
    const hasDownloadPermission = await this.checkDownloadPermission();
    const controlButtons = document.getElementById('control-buttons');
    
    if (!controlButtons) return;
    
    if (hasDownloadPermission) {
      // Get the full-size image URL by modifying the current src
      const fullSizeUrl = this.mainImage.src.replace('?size=medium', '');
      
      controlButtons.innerHTML = `
        <a href="${fullSizeUrl}" target="_blank" class="btn">View Full Size</a>
        <a href="${fullSizeUrl}" download="${this.config.imageName}" class="btn">Download</a>
      `;
      
      this.mainImage.addEventListener('click', () => {
        window.open(fullSizeUrl, '_blank');
      });
      this.mainImage.style.cursor = 'pointer';
    } else {
      controlButtons.innerHTML = `
        <a href="${this.mainImage.src}" target="_blank" class="btn">View Medium Size</a>
        <button class="btn" onclick="requestDownloadAccess()">Request Download Access</button>
      `;
      
      this.mainImage.addEventListener('click', () => {
        window.open(this.mainImage.src, '_blank');
      });
      this.mainImage.style.cursor = 'pointer';
    }
  }

  private setupKeyboardNavigation(): void {
    document.addEventListener('keydown', (e) => {
      // Previous image
      if (e.key === 'ArrowLeft' && this.config.prevImagePath) {
        e.preventDefault();
        window.location.href = `${this.config.galleryUrl}/detail/${this.config.prevImagePath}`;
      }
      
      // Next image
      if (e.key === 'ArrowRight' && this.config.nextImagePath) {
        e.preventDefault();
        window.location.href = `${this.config.galleryUrl}/detail/${this.config.nextImagePath}`;
      }
      
      // ESC to go back to gallery
      if (e.key === 'Escape') {
        e.preventDefault();
        const pathParts = this.config.imagePath.split('/');
        const galleryPath = pathParts.slice(0, -1).join('/');
        window.location.href = `${this.config.galleryUrl}/${galleryPath}`;
      }
    });
  }

  private preloadAdjacentImages(): void {
    // Preload adjacent images for faster navigation
    if (this.config.prevImagePath) {
      const prevImg = new Image();
      prevImg.src = `${this.config.galleryUrl}/image/${this.config.prevImagePath}?size=medium`;
    }
    
    if (this.config.nextImagePath) {
      const nextImg = new Image();
      nextImg.src = `${this.config.galleryUrl}/image/${this.config.nextImagePath}?size=medium`;
    }
  }
}

// Global function for legacy compatibility
declare global {
  interface Window {
    requestDownloadAccess: () => void;
  }
}

// Function to request download access - redirect to login
window.requestDownloadAccess = function() {
  window.location.href = '/_login';
};