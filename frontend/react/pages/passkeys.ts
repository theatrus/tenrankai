import { PasskeyManager, WebAuthnUtils } from '../auth/webauthn.ts';
import type { PasskeyInfo } from '../auth/webauthn.ts';
import { Toast } from '../ui/toast.ts';

function createMessage(tagName: 'div' | 'p', className: string, text: string): HTMLElement {
  const element = document.createElement(tagName);
  element.className = className;
  element.textContent = text;
  return element;
}

function createPasskeyItem(passkey: PasskeyInfo): HTMLElement {
  const item = document.createElement('div');
  item.className = 'passkey-item';
  item.dataset.id = passkey.id;

  const info = document.createElement('div');
  info.className = 'passkey-info';

  const name = document.createElement('div');
  name.className = 'passkey-name';
  name.textContent = passkey.name;

  const meta = document.createElement('div');
  meta.className = 'passkey-meta';
  meta.textContent = `Created: ${PasskeyManager.formatDate(passkey.created_at)}`;
  if (passkey.last_used_at) {
    meta.append(` \u2022 Last used: ${PasskeyManager.formatDate(passkey.last_used_at)}`);
  }

  info.append(name, meta);

  const deleteButton = document.createElement('button');
  deleteButton.className = 'btn-delete';
  deleteButton.dataset.passkeyId = passkey.id;
  deleteButton.textContent = 'Delete';
  deleteButton.addEventListener('click', () => {
    void handleDeletePasskey(passkey.id);
  });

  item.append(info, deleteButton);
  return item;
}

async function refreshPasskeysList(): Promise<void> {
  const listContainer = document.getElementById('passkeysList');
  if (!listContainer) {
    return;
  }

  try {
    const passkeys = await PasskeyManager.loadPasskeys();

    if (passkeys.length === 0) {
      listContainer.replaceChildren(createMessage('p', 'no-passkeys', 'No passkeys registered yet.'));
    } else {
      listContainer.replaceChildren(...passkeys.map((passkey) => createPasskeyItem(passkey)));
    }
  } catch {
    listContainer.replaceChildren(createMessage('p', 'error', 'Failed to load passkeys'));
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
      addPasskeySection.replaceChildren(createMessage('div', 'message info', 'Your browser does not support passkeys.'));
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
