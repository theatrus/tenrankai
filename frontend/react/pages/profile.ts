import { PasskeyManager } from '../auth/webauthn.ts';
import type { PasskeyInfo } from '../auth/webauthn.ts';

class ProfilePasskeys {
  private passkeyList: HTMLElement | null;

  constructor() {
    this.passkeyList = document.getElementById('passkeyList');
    const enrollButton = document.getElementById('enrollNewPasskey');
    enrollButton?.addEventListener('click', () => this.enrollNewPasskey());

    if (this.passkeyList) {
      void this.loadPasskeyList();
    }
  }

  private async loadPasskeyList(): Promise<void> {
    if (!this.passkeyList) {
      return;
    }

    try {
      const passkeys = await PasskeyManager.loadPasskeys();

      if (passkeys.length === 0) {
        this.passkeyList.replaceChildren(this.createMessage('loading', 'No passkeys found'));
        return;
      }

      this.passkeyList.replaceChildren(...passkeys.map((passkey) => this.createPasskeyItem(passkey)));
    } catch (error) {
      console.error('Error loading passkeys:', error);
      this.passkeyList.replaceChildren(this.createMessage('error', 'Failed to load passkeys'));
    }
  }

  private createPasskeyItem(passkey: PasskeyInfo): HTMLElement {
    const item = document.createElement('div');
    item.className = 'passkey-item';
    item.dataset.id = passkey.id;

    const info = document.createElement('div');
    info.className = 'passkey-info';

    const name = document.createElement('div');
    name.className = 'passkey-name';
    name.textContent = passkey.name || 'Unnamed Passkey';

    const created = document.createElement('div');
    created.className = 'passkey-created';
    created.textContent = `Created: ${PasskeyManager.formatDate(passkey.created_at)}`;

    info.append(name, created);

    const actions = document.createElement('div');
    actions.className = 'passkey-actions';

    const removeButton = document.createElement('button');
    removeButton.type = 'button';
    removeButton.className = 'btn-danger';
    removeButton.dataset.passkeyId = passkey.id;
    removeButton.textContent = 'Remove';
    removeButton.addEventListener('click', () => {
      void this.removePasskey(passkey.id);
    });

    actions.append(removeButton);
    item.append(info, actions);

    return item;
  }

  private createMessage(className: string, text: string): HTMLElement {
    const message = document.createElement('div');
    message.className = className;
    message.textContent = text;
    return message;
  }

  private async removePasskey(passkeyId: string): Promise<void> {
    if (!confirm('Are you sure you want to remove this passkey? This action cannot be undone.')) {
      return;
    }

    try {
      await PasskeyManager.deletePasskey(passkeyId);
      document.querySelector(`[data-id="${CSS.escape(passkeyId)}"]`)?.remove();

      if (document.querySelectorAll('.passkey-item').length === 0) {
        window.location.reload();
      }
    } catch (error) {
      console.error('Error removing passkey:', error);
      alert('Failed to remove passkey. Please try again.');
    }
  }

  private enrollNewPasskey(): void {
    const returnUrl = encodeURIComponent('/_login/profile');
    window.location.href = `/_login/passkey-enrollment?return=${returnUrl}`;
  }
}

document.addEventListener('DOMContentLoaded', () => {
  if (document.querySelector('#passkeyList, #enrollNewPasskey')) {
    new ProfilePasskeys();
  }
});
