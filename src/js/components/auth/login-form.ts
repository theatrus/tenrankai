import { ApiClient } from '../../core/api-client.js';
import { WebAuthnUtils } from './webauthn.js';
import { AuthUtils } from './auth-utils.js';
import type { LoginCredentials } from '../../core/types.js';

export class LoginForm {
  private usernameInput: HTMLInputElement;
  private submitButton: HTMLButtonElement;
  private isAuthenticating = false;

  constructor(formElement: HTMLFormElement) {
    this.usernameInput = formElement.querySelector('#username') as HTMLInputElement;
    this.submitButton = formElement.querySelector('button[type="submit"]') as HTMLButtonElement;
    
    if (!this.usernameInput || !this.submitButton) {
      throw new Error('Required form elements not found');
    }

    this.setupEventListeners(formElement);
  }

  private setupEventListeners(formElement: HTMLFormElement): void {
    formElement.addEventListener('submit', this.handleSubmit.bind(this));
  }

  private async handleSubmit(event: Event): Promise<void> {
    event.preventDefault();
    
    if (this.isAuthenticating) {
      return;
    }

    const username = this.usernameInput.value.trim();
    if (!username) {
      AuthUtils.showError('Please enter your username or email');
      return;
    }

    this.isAuthenticating = true;
    this.setLoadingState(true);
    AuthUtils.hideError();

    try {
      // Check if WebAuthn is supported and user has passkeys
      if (WebAuthnUtils.isSupported()) {
        const passkeyCheck = await WebAuthnUtils.checkUserHasPasskeys(username);
        
        if (passkeyCheck.has_passkeys) {
          // Try passkey authentication first
          try {
            const response = await WebAuthnUtils.authenticateWithPasskey(username);
            const result = await response.json();
            
            if (result.success) {
              this.handleLoginSuccess(result);
              return;
            }
          } catch (passkeyError) {
            console.log('Passkey authentication failed, falling back to email:', passkeyError);
            // Fall through to email authentication
          }
        }
      }

      // Fall back to email authentication
      const credentials: LoginCredentials = { username };
      const result = await ApiClient.requestEmailLogin(credentials);
      
      if (result.success) {
        AuthUtils.showSuccess(result.message || 'Check your email for a login link');
        this.submitButton.textContent = 'Email Sent';
        this.submitButton.disabled = true;
      } else {
        AuthUtils.showError(result.message || 'Login failed');
      }

    } catch (error) {
      console.error('Login error:', error);
      const errorMessage = error instanceof Error ? error.message : 'An unexpected error occurred';
      AuthUtils.showError(errorMessage);
    } finally {
      this.isAuthenticating = false;
      this.setLoadingState(false);
    }
  }

  private setLoadingState(loading: boolean): void {
    this.submitButton.disabled = loading;
    this.submitButton.textContent = loading ? 'Authenticating...' : 'Sign In';
    this.usernameInput.disabled = loading;
  }

  private handleLoginSuccess(result: { redirect?: string }): void {
    // Check for return URL
    const returnUrl = AuthUtils.getReturnUrl();
    const redirectUrl = result.redirect || returnUrl || '/';
    
    AuthUtils.showSuccess('Authentication successful! Redirecting...');
    
    // Redirect after a short delay
    setTimeout(() => {
      window.location.href = redirectUrl;
    }, 1000);
  }
}