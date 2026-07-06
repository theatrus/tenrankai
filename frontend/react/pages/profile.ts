import { AuthUtils } from '../auth/auth-utils.ts';
import { PasskeyManager } from '../auth/webauthn.ts';

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
        this.passkeyList.innerHTML = '<div class="loading">No passkeys found</div>';
        return;
      }

      this.passkeyList.innerHTML = passkeys.map((passkey) => `
        <div class="passkey-item" data-id="${AuthUtils.escapeHtml(passkey.id)}">
          <div class="passkey-info">
            <div class="passkey-name">${AuthUtils.escapeHtml(passkey.name || 'Unnamed Passkey')}</div>
            <div class="passkey-created">Created: ${PasskeyManager.formatDate(passkey.created_at)}</div>
          </div>
          <div class="passkey-actions">
            <button type="button" class="btn-danger" data-passkey-id="${AuthUtils.escapeHtml(passkey.id)}">Remove</button>
          </div>
        </div>
      `).join('');

      this.passkeyList.querySelectorAll<HTMLElement>('[data-passkey-id]').forEach((button) => {
        const passkeyId = button.dataset.passkeyId;
        if (passkeyId) {
          button.addEventListener('click', () => {
            void this.removePasskey(passkeyId);
          });
        }
      });
    } catch (error) {
      console.error('Error loading passkeys:', error);
      this.passkeyList.innerHTML = '<div class="error">Failed to load passkeys</div>';
    }
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
