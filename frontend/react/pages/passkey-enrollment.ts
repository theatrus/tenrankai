import { WebAuthnUtils } from '../auth/webauthn.ts';
import { Toast } from '../ui/toast.ts';

document.addEventListener('DOMContentLoaded', () => {
  const button = document.getElementById('setupPasskeyBtn') as HTMLButtonElement | null;
  if (!button) {
    return;
  }

  const setupNote = document.querySelector<HTMLElement>('.setup-note');

  if (!WebAuthnUtils.isSupported()) {
    button.disabled = true;
    button.textContent = 'Passkeys not supported';
    if (setupNote) {
      setupNote.textContent = 'Your browser does not support passkeys.';
    }
  }

  button.addEventListener('click', async () => {
    const redirectUrl = button.dataset.redirectUrl || '/gallery';
    button.disabled = true;
    button.textContent = 'Setting up...';

    try {
      await WebAuthnUtils.registerPasskey('Passkey');
      Toast.success('Passkey set up successfully! Redirecting...');

      setTimeout(() => {
        window.location.href = redirectUrl;
      }, 1500);
    } catch (error) {
      if (error instanceof Error && error.name === 'NotAllowedError') {
        Toast.error('Passkey setup was cancelled or timed out. Please try again.');
      } else {
        Toast.error(error instanceof Error ? error.message : 'Failed to set up passkey. Please try again.');
      }

      button.disabled = false;
      button.textContent = 'Set up passkey';
    }
  });
});
