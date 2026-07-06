import { AuthUtils } from '../auth/auth-utils.ts';
import { WebAuthnUtils } from '../auth/webauthn.ts';

function getElement<T extends HTMLElement>(id: string): T | null {
  return document.getElementById(id) as T | null;
}

async function handleLoginSubmit(event: SubmitEvent): Promise<void> {
  event.preventDefault();

  const form = event.currentTarget as HTMLFormElement;
  const usernameInput = getElement<HTMLInputElement>('username');
  const messageDiv = getElement<HTMLElement>('message');
  const errorDiv = getElement<HTMLElement>('errorMessage');
  const submitButton = form.querySelector<HTMLButtonElement>('button[type="submit"]');

  if (!usernameInput || !messageDiv || !errorDiv || !submitButton) {
    return;
  }

  const username = usernameInput.value.trim();
  AuthUtils.hideError();

  if (!username) {
    AuthUtils.showError('Please enter a username or email address');
    return;
  }

  submitButton.disabled = true;
  submitButton.textContent = 'Checking...';

  try {
    if (WebAuthnUtils.isSupported()) {
      const { has_passkeys: hasPasskeys } = await WebAuthnUtils.checkUserHasPasskeys(username);

      if (hasPasskeys) {
        AuthUtils.hideElement('loginFormContainer');
        AuthUtils.showElement('authenticatingContainer');

        try {
          await WebAuthnUtils.authenticateWithPasskey(username);
          const returnUrl = AuthUtils.getReturnUrl();
          window.location.href = returnUrl || '/gallery';
          return;
        } catch {
          AuthUtils.hideElement('authenticatingContainer');
          AuthUtils.showElement('loginFormContainer');

          errorDiv.textContent = 'Passkey authentication cancelled. Sending login email instead...';
          errorDiv.style.display = 'block';
          errorDiv.classList.add('info');

          await new Promise((resolve) => {
            setTimeout(resolve, 1000);
          });
        }
      }
    }

    submitButton.textContent = 'Sending...';
    AuthUtils.hideError();
    errorDiv.classList.remove('info');

    const response = await fetch('/_login/request', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ username }),
    });

    const data = await response.json();

    if (data.success) {
      AuthUtils.hideElement('loginFormContainer');
      AuthUtils.hideElement('authenticatingContainer');
      messageDiv.textContent = data.message;
      AuthUtils.showElement('successContainer');
    } else {
      AuthUtils.showError(data.message || 'Login failed');
    }
  } catch {
    AuthUtils.hideElement('authenticatingContainer');
    AuthUtils.showElement('loginFormContainer');
    AuthUtils.showError('An error occurred. Please try again.');
  } finally {
    submitButton.disabled = false;
    submitButton.textContent = 'Continue';
  }
}

document.addEventListener('DOMContentLoaded', () => {
  const loginForm = getElement<HTMLFormElement>('loginForm');
  loginForm?.addEventListener('submit', (event) => {
    void handleLoginSubmit(event as SubmitEvent);
  });
});
