import { AuthUtils } from '../auth/auth-utils.ts';
import { PasskeyManager, WebAuthnUtils } from '../auth/webauthn.ts';
import { Toast } from '../ui/toast.ts';

async function refreshPasskeysList(): Promise<void> {
  const listContainer = document.getElementById('passkeysList');
  if (!listContainer) {
    return;
  }

  try {
    const passkeys = await PasskeyManager.loadPasskeys();

    if (passkeys.length === 0) {
      listContainer.innerHTML = '<p class="no-passkeys">No passkeys registered yet.</p>';
    } else {
      listContainer.innerHTML = passkeys.map((passkey) => `
        <div class="passkey-item" data-id="${AuthUtils.escapeHtml(passkey.id)}">
          <div class="passkey-info">
            <div class="passkey-name">${AuthUtils.escapeHtml(passkey.name)}</div>
            <div class="passkey-meta">
              Created: ${PasskeyManager.formatDate(passkey.created_at)}
              ${passkey.last_used_at ? `\u2022 Last used: ${PasskeyManager.formatDate(passkey.last_used_at)}` : ''}
            </div>
          </div>
          <button class="btn-delete" data-passkey-id="${AuthUtils.escapeHtml(passkey.id)}">Delete</button>
        </div>
      `).join('');

      listContainer.querySelectorAll<HTMLElement>('[data-passkey-id]').forEach((button) => {
        const passkeyId = button.dataset.passkeyId;
        if (passkeyId) {
          button.addEventListener('click', () => {
            void handleDeletePasskey(passkeyId);
          });
        }
      });
    }
  } catch {
    listContainer.innerHTML = '<p class="error">Failed to load passkeys</p>';
  }
}

async function handleDeletePasskey(passkeyId: string): Promise<void> {
  if (!confirm('Are you sure you want to delete this passkey?')) {
    return;
  }

  try {
    await PasskeyManager.deletePasskey(passkeyId);
    Toast.success('Passkey deleted successfully!');
    await refreshPasskeysList();
  } catch {
    Toast.error('Failed to delete passkey. Please try again.');
  }
}

document.addEventListener('DOMContentLoaded', () => {
  const addPasskeyForm = document.getElementById('addPasskeyForm') as HTMLFormElement | null;
  if (!addPasskeyForm) {
    return;
  }

  if (!WebAuthnUtils.isSupported()) {
    const addPasskeySection = document.querySelector<HTMLElement>('.add-passkey-section');
    if (addPasskeySection) {
      addPasskeySection.innerHTML = '<div class="message info">Your browser does not support passkeys.</div>';
    }
  }

  addPasskeyForm.addEventListener('submit', async (event) => {
    event.preventDefault();

    const nameInput = document.getElementById('passkeyName') as HTMLInputElement | null;
    const submitButton = addPasskeyForm.querySelector<HTMLButtonElement>('button[type="submit"]');
    const name = nameInput?.value.trim() || '';

    if (!nameInput || !submitButton) {
      return;
    }

    if (!name) {
      Toast.error('Please enter a name for the passkey');
      return;
    }

    submitButton.disabled = true;
    submitButton.textContent = 'Adding...';

    try {
      await WebAuthnUtils.registerPasskey(name);
      Toast.success('Passkey added successfully!');
      nameInput.value = '';
      await refreshPasskeysList();
    } catch (error) {
      Toast.error(error instanceof Error ? error.message : 'Failed to add passkey. Please try again.');
    } finally {
      submitButton.disabled = false;
      submitButton.textContent = 'Add Passkey';
    }
  });

  void refreshPasskeysList();
});
