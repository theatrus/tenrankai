// Authentication utility functions

export class AuthUtils {
  /**
   * HTML escape for security
   */
  static escapeHtml(text: string): string {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }

  /**
   * Get cookie value by name
   */
  static getCookie(name: string): string | null {
    const cookies = document.cookie.split(';');
    for (const cookie of cookies) {
        const [cookieName, cookieValue] = cookie.trim().split('=');
        if (cookieName === name && cookieValue) {
            return decodeURIComponent(cookieValue);
        }
    }
    return null;
  }

  /**
   * Get return URL from cookie
   */
  static getReturnUrl(): string | null {
    return this.getCookie('return_url');
  }

  /**
   * Show element by ID
   */
  static showElement(elementId: string): void {
    const element = document.getElementById(elementId);
    if (element) {
        element.style.display = 'block';
    }
  }

  /**
   * Hide element by ID
   */
  static hideElement(elementId: string): void {
    const element = document.getElementById(elementId);
    if (element) {
        element.style.display = 'none';
    }
  }

  /**
   * Show error message
   */
  static showError(message: string, elementId: string = 'errorMessage'): void {
    const errorDiv = document.getElementById(elementId);
    if (errorDiv) {
        errorDiv.textContent = message;
        errorDiv.style.display = 'block';
    }
  }

  /**
   * Hide error message
   */
  static hideError(elementId: string = 'errorMessage'): void {
    const errorDiv = document.getElementById(elementId);
    if (errorDiv) {
        errorDiv.style.display = 'none';
    }
  }

  /**
   * Show success message
   */
  static showSuccess(message: string, elementId: string = 'successMessage'): void {
    const successDiv = document.getElementById(elementId);
    if (successDiv) {
        successDiv.textContent = message;
        successDiv.style.display = 'block';
    }
  }

  /**
   * Hide success message
   */
  static hideSuccess(elementId: string = 'successMessage'): void {
    const successDiv = document.getElementById(elementId);
    if (successDiv) {
        successDiv.style.display = 'none';
    }
  }
}