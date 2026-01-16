import { DomUtils } from '../../core/dom-utils.js';
import type { ToastOptions } from '../../core/types.js';

export class Toast {
  private static container?: HTMLElement;
  private element: HTMLElement;
  private timeout?: number;

  constructor(message: string, options: ToastOptions = { type: 'info' }) {
    this.element = this.createElement(message, options);
    this.show(options);
  }

  private createElement(message: string, options: ToastOptions): HTMLElement {
    return DomUtils.createElement('div', {
      class: `toast toast-${options.type}`,
      role: 'alert',
      'aria-live': 'polite'
    }, [
      DomUtils.createElement('div', { class: 'toast-content' }, [message]),
      DomUtils.createElement('button', {
        class: 'toast-close',
        type: 'button',
        'aria-label': 'Close'
      }, ['×'])
    ]);
  }

  private show(options: ToastOptions): void {
    // Create container if it doesn't exist
    if (!Toast.container) {
      Toast.container = DomUtils.createElement('div', {
        class: 'toast-container',
        'aria-label': 'Notifications'
      });
      document.body.appendChild(Toast.container);
    }

    // Add close event listener
    const closeButton = this.element.querySelector('.toast-close') as HTMLButtonElement;
    if (closeButton) {
      closeButton.addEventListener('click', () => this.hide());
    }

    // Add to container
    Toast.container.appendChild(this.element);

    // Trigger animation
    requestAnimationFrame(() => {
      this.element.classList.add('toast-show');
    });

    // Auto-hide unless persistent
    if (!options.persistent) {
      const duration = options.duration ?? this.getDefaultDuration(options.type);
      this.timeout = window.setTimeout(() => this.hide(), duration);
    }

    // Hide on click (except close button)
    this.element.addEventListener('click', (e) => {
      if (e.target !== closeButton) {
        this.hide();
      }
    });
  }

  private getDefaultDuration(type: ToastOptions['type']): number {
    switch (type) {
      case 'error': return 8000;
      case 'warning': return 6000;
      case 'success': return 4000;
      case 'info': 
      default: return 5000;
    }
  }

  public hide(): void {
    if (this.timeout) {
      clearTimeout(this.timeout);
    }

    this.element.classList.remove('toast-show');
    this.element.classList.add('toast-hide');

    // Remove from DOM after animation
    setTimeout(() => {
      if (this.element.parentNode) {
        this.element.parentNode.removeChild(this.element);
      }

      // Clean up container if empty
      if (Toast.container && Toast.container.children.length === 0) {
        Toast.container.remove();
        Toast.container = undefined;
      }
    }, 300);
  }

  // Static convenience methods
  static success(message: string, options?: Omit<ToastOptions, 'type'>): Toast {
    return new Toast(message, { ...options, type: 'success' });
  }

  static error(message: string, options?: Omit<ToastOptions, 'type'>): Toast {
    return new Toast(message, { ...options, type: 'error' });
  }

  static warning(message: string, options?: Omit<ToastOptions, 'type'>): Toast {
    return new Toast(message, { ...options, type: 'warning' });
  }

  static info(message: string, options?: Omit<ToastOptions, 'type'>): Toast {
    return new Toast(message, { ...options, type: 'info' });
  }

  // Clear all toasts
  static clearAll(): void {
    if (Toast.container) {
      const toasts = Array.from(Toast.container.querySelectorAll('.toast'));
      toasts.forEach(toast => {
        toast.classList.remove('toast-show');
        toast.classList.add('toast-hide');
      });

      setTimeout(() => {
        if (Toast.container) {
          Toast.container.remove();
          Toast.container = undefined;
        }
      }, 300);
    }
  }
}