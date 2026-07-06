type ToastType = 'success' | 'error' | 'info' | 'warning';

interface ToastOptions {
  type: ToastType;
  duration?: number;
  persistent?: boolean;
}

export class Toast {
  private static container?: HTMLElement;
  private element: HTMLElement;
  private timeout?: number;

  constructor(message: string, options: ToastOptions = { type: 'info' }) {
    this.element = this.createElement(message, options);
    this.show(options);
  }

  private createElement(message: string, options: ToastOptions): HTMLElement {
    const element = document.createElement('div');
    element.className = `toast toast-${options.type}`;
    element.setAttribute('role', 'alert');
    element.setAttribute('aria-live', 'polite');

    const content = document.createElement('div');
    content.className = 'toast-content';
    content.textContent = message;

    const close = document.createElement('button');
    close.className = 'toast-close';
    close.type = 'button';
    close.setAttribute('aria-label', 'Close');
    close.textContent = '\u00d7';

    element.append(content, close);
    return element;
  }

  private show(options: ToastOptions): void {
    if (!Toast.container) {
      Toast.container = document.createElement('div');
      Toast.container.className = 'toast-container';
      Toast.container.setAttribute('aria-label', 'Notifications');
      document.body.appendChild(Toast.container);
    }

    const closeButton = this.element.querySelector('.toast-close');
    closeButton?.addEventListener('click', () => this.hide());

    Toast.container.appendChild(this.element);
    requestAnimationFrame(() => {
      this.element.classList.add('toast-show');
    });

    if (!options.persistent) {
      const duration = options.duration ?? this.getDefaultDuration(options.type);
      this.timeout = window.setTimeout(() => this.hide(), duration);
    }

    this.element.addEventListener('click', (event) => {
      if (event.target !== closeButton) {
        this.hide();
      }
    });
  }

  private getDefaultDuration(type: ToastType): number {
    switch (type) {
      case 'error':
        return 8000;
      case 'warning':
        return 6000;
      case 'success':
        return 4000;
      case 'info':
      default:
        return 5000;
    }
  }

  public hide(): void {
    if (this.timeout) {
      clearTimeout(this.timeout);
    }

    this.element.classList.remove('toast-show');
    this.element.classList.add('toast-hide');

    setTimeout(() => {
      this.element.remove();

      if (Toast.container && Toast.container.children.length === 0) {
        Toast.container.remove();
        Toast.container = undefined;
      }
    }, 300);
  }

  static success(message: string, options?: Omit<ToastOptions, 'type'>): Toast {
    return new Toast(message, { ...options, type: 'success' });
  }

  static error(message: string, options?: Omit<ToastOptions, 'type'>): Toast {
    return new Toast(message, { ...options, type: 'error' });
  }
}
