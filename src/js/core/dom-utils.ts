// DOM utility functions for common operations

export class DomUtils {
  /**
   * Safely query selector with type assertion
   */
  static querySelector<T extends HTMLElement>(
    selector: string,
    context: Document | HTMLElement = document
  ): T | null {
    return context.querySelector(selector) as T | null;
  }

  /**
   * Safely query selector with error if not found
   */
  static requireElement<T extends HTMLElement>(
    selector: string,
    context: Document | HTMLElement = document
  ): T {
    const element = this.querySelector<T>(selector, context);
    if (!element) {
      throw new Error(`Required element not found: ${selector}`);
    }
    return element;
  }

  /**
   * Get element by ID with type assertion
   */
  static getElementById<T extends HTMLElement>(id: string): T | null {
    return document.getElementById(id) as T | null;
  }

  /**
   * Add event listener with proper cleanup
   */
  static addEventListener<K extends keyof HTMLElementEventMap>(
    element: HTMLElement,
    type: K,
    listener: (this: HTMLElement, ev: HTMLElementEventMap[K]) => any,
    options?: boolean | AddEventListenerOptions
  ): () => void {
    element.addEventListener(type, listener, options);
    return () => element.removeEventListener(type, listener, options);
  }

  /**
   * Debounce function calls
   */
  static debounce<T extends (...args: any[]) => any>(
    func: T,
    wait: number
  ): (...args: Parameters<T>) => void {
    let timeout: ReturnType<typeof setTimeout>;
    
    return (...args: Parameters<T>) => {
      clearTimeout(timeout);
      timeout = setTimeout(() => func.apply(this, args), wait);
    };
  }

  /**
   * Check if element is visible in viewport
   */
  static isInViewport(element: HTMLElement, threshold: number = 0): boolean {
    const rect = element.getBoundingClientRect();
    const viewHeight = Math.max(document.documentElement.clientHeight, window.innerHeight);
    const viewWidth = Math.max(document.documentElement.clientWidth, window.innerWidth);

    return !(
      rect.bottom < threshold ||
      rect.right < threshold ||
      rect.top > viewHeight - threshold ||
      rect.left > viewWidth - threshold
    );
  }

  /**
   * Wait for images to load
   */
  static async waitForImages(container: HTMLElement, timeout: number = 5000): Promise<void> {
    const images = container.querySelectorAll('img');
    const imagePromises = Array.from(images).map(img => {
      if (img.complete && img.naturalWidth > 0) {
        return Promise.resolve();
      }
      
      return new Promise<void>((resolve) => {
        const cleanup = () => {
          img.removeEventListener('load', onLoad);
          img.removeEventListener('error', onError);
          resolve();
        };

        const onLoad = () => cleanup();
        const onError = () => cleanup();

        img.addEventListener('load', onLoad);
        img.addEventListener('error', onError);
        
        // Timeout fallback
        setTimeout(() => cleanup(), timeout);
      });
    });

    await Promise.all(imagePromises);
  }

  /**
   * Create element with attributes and children
   */
  static createElement<K extends keyof HTMLElementTagNameMap>(
    tagName: K,
    attributes: Record<string, string> = {},
    children: (HTMLElement | string)[] = []
  ): HTMLElementTagNameMap[K] {
    const element = document.createElement(tagName);
    
    Object.entries(attributes).forEach(([key, value]) => {
      element.setAttribute(key, value);
    });
    
    children.forEach(child => {
      if (typeof child === 'string') {
        element.appendChild(document.createTextNode(child));
      } else {
        element.appendChild(child);
      }
    });
    
    return element;
  }
}