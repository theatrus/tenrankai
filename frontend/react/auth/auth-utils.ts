export class AuthUtils {
  static escapeHtml(text: string): string {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }

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

  static getReturnUrl(): string | null {
    return this.getCookie('return_url');
  }

  static showElement(elementId: string): void {
    const element = document.getElementById(elementId);
    if (element) {
      element.style.display = 'block';
    }
  }

  static hideElement(elementId: string): void {
    const element = document.getElementById(elementId);
    if (element) {
      element.style.display = 'none';
    }
  }

  static showError(message: string, elementId = 'errorMessage'): void {
    const errorDiv = document.getElementById(elementId);
    if (errorDiv) {
      errorDiv.textContent = message;
      errorDiv.style.display = 'block';
    }
  }

  static hideError(elementId = 'errorMessage'): void {
    const errorDiv = document.getElementById(elementId);
    if (errorDiv) {
      errorDiv.style.display = 'none';
    }
  }

  static showSuccess(message: string, elementId = 'successMessage'): void {
    const successDiv = document.getElementById(elementId);
    if (successDiv) {
      successDiv.textContent = message;
      successDiv.style.display = 'block';
    }
  }
}
