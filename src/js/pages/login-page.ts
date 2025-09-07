import { LoginForm } from '../components/auth/login-form.js';
import { PasskeyManager, WebAuthnUtils } from '../components/auth/webauthn.js';
import { AuthUtils } from '../components/auth/auth-utils.js';
import { DomUtils } from '../core/dom-utils.js';

export class LoginPage {
  private loginForm?: LoginForm;

  constructor() {
    this.init();
  }

  private init(): void {
    // Initialize login form if present
    const loginFormElement = DomUtils.querySelector<HTMLFormElement>('#loginForm');
    if (loginFormElement) {
      this.loginForm = new LoginForm(loginFormElement);
    }

    // Initialize profile page if present
    this.initProfilePage();
  }

  private initProfilePage(): void {
    // Load passkeys if the passkey list element exists
    const passkeyList = DomUtils.getElementById('passkeyList');
    if (passkeyList) {
      this.loadPasskeyList();
    }

    // Setup new passkey enrollment button
    const enrollButton = DomUtils.getElementById('enrollNewPasskey');
    if (enrollButton) {
      enrollButton.addEventListener('click', this.enrollNewPasskey.bind(this));
    }
  }

  private async loadPasskeyList(): Promise<void> {
    const passkeyList = DomUtils.getElementById('passkeyList');
    if (!passkeyList) return;
    
    try {
      const passkeys = await PasskeyManager.loadPasskeys();
      
      if (passkeys.length === 0) {
        passkeyList.innerHTML = '<div class="loading">No passkeys found</div>';
        return;
      }
      
      passkeyList.innerHTML = passkeys.map(passkey => `
        <div class="passkey-item" data-id="${passkey.id}">
          <div class="passkey-info">
            <div class="passkey-name">${AuthUtils.escapeHtml(passkey.name || 'Unnamed Passkey')}</div>
            <div class="passkey-created">Created: ${PasskeyManager.formatDate(passkey.created_at)}</div>
          </div>
          <div class="passkey-actions">
            <button type="button" class="btn-danger" data-passkey-id="${passkey.id}">Remove</button>
          </div>
        </div>
      `).join('');
      
      // Add event listeners for remove buttons
      const removeButtons = passkeyList.querySelectorAll('[data-passkey-id]');
      removeButtons.forEach(button => {
        const passkeyId = button.getAttribute('data-passkey-id');
        if (passkeyId) {
          button.addEventListener('click', () => this.removePasskey(passkeyId));
        }
      });
      
    } catch (error) {
      console.error('Error loading passkeys:', error);
      passkeyList.innerHTML = '<div class="error">Failed to load passkeys</div>';
    }
  }

  private async removePasskey(passkeyId: string): Promise<void> {
    if (!confirm('Are you sure you want to remove this passkey? This action cannot be undone.')) {
      return;
    }
    
    try {
      await PasskeyManager.deletePasskey(passkeyId);
      
      // Remove the passkey item from the DOM
      const passkeyItem = document.querySelector(`[data-id="${passkeyId}"]`);
      if (passkeyItem) {
        passkeyItem.remove();
      }
      
      // Check if this was the last passkey
      const remainingPasskeys = document.querySelectorAll('.passkey-item');
      if (remainingPasskeys.length === 0) {
        // Reload the page to show the "no passkeys" state
        window.location.reload();
      }
      
    } catch (error) {
      console.error('Error removing passkey:', error);
      alert('Failed to remove passkey. Please try again.');
    }
  }

  private async enrollNewPasskey(): Promise<void> {
    try {
      // Redirect to enrollment page
      const returnUrl = encodeURIComponent('/_login/profile');
      window.location.href = `/_login/passkey-enrollment?return=${returnUrl}`;
      
    } catch (error) {
      console.error('Error starting passkey enrollment:', error);
      alert('Failed to start passkey enrollment. Please try again.');
    }
  }
}

// Auto-initialize on DOMContentLoaded
document.addEventListener('DOMContentLoaded', () => {
  // Check if we're on a login-related page
  const isLoginPage = document.querySelector('#loginForm, #passkeyList') !== null;
  
  if (isLoginPage) {
    new LoginPage();
  }
});

// Legacy support - maintain window.LoginUtils for existing templates
declare global {
  interface Window {
    LoginUtils?: any;
  }
}

// Provide legacy compatibility
window.LoginUtils = {
  ...AuthUtils,
  ...WebAuthnUtils,
  ...PasskeyManager,
  initProfilePage: () => new LoginPage()
};