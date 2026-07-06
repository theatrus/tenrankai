export class WebAuthnUtils {
  static base64ToArrayBuffer(base64: string): ArrayBuffer {
    const standardBase64 = base64.replace(/-/g, '+').replace(/_/g, '/');
    const padding = standardBase64.length % 4;
    const paddedBase64 = padding ? standardBase64 + '===='.substring(padding) : standardBase64;

    const binaryString = window.atob(paddedBase64);
    const bytes = new Uint8Array(binaryString.length);
    for (let i = 0; i < binaryString.length; i += 1) {
      bytes[i] = binaryString.charCodeAt(i);
    }
    return bytes.buffer;
  }

  static arrayBufferToBase64(buffer: ArrayBuffer): string {
    const bytes = new Uint8Array(buffer);
    let binary = '';
    for (let i = 0; i < bytes.byteLength; i += 1) {
      binary += String.fromCharCode(bytes[i]);
    }
    return window.btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=/g, '');
  }

  static isSupported(): boolean {
    return Boolean(window.PublicKeyCredential);
  }

  static async checkUserHasPasskeys(username: string): Promise<{ has_passkeys: boolean }> {
    const response = await fetch('/api/webauthn/check-passkeys', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      credentials: 'same-origin',
      body: JSON.stringify({ username }),
    });

    if (response.ok) {
      return await response.json();
    }

    return { has_passkeys: false };
  }

  static async authenticateWithPasskey(username: string): Promise<Response> {
    const startResponse = await fetch('/api/webauthn/authenticate/start', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      credentials: 'same-origin',
      body: JSON.stringify({ username }),
    });

    if (!startResponse.ok) {
      throw new Error('Failed to start authentication');
    }

    const response = await startResponse.json();
    const options = response.publicKey || response;
    const authId = response.auth_id;

    options.challenge = this.base64ToArrayBuffer(options.challenge);

    if (options.allowCredentials) {
      options.allowCredentials = options.allowCredentials.map((cred: PublicKeyCredentialDescriptor) => ({
        ...cred,
        id: this.base64ToArrayBuffer(cred.id as unknown as string),
      }));
    }

    const credential = await navigator.credentials.get({
      publicKey: options,
    }) as PublicKeyCredential;

    if (!credential) {
      throw new Error('No credential obtained');
    }

    const authResponse = credential.response as AuthenticatorAssertionResponse;
    const finishResponse = await fetch(`/api/webauthn/authenticate/finish/${authId}`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      credentials: 'same-origin',
      body: JSON.stringify({
        id: credential.id,
        rawId: this.arrayBufferToBase64(credential.rawId),
        response: {
          authenticatorData: this.arrayBufferToBase64(authResponse.authenticatorData),
          clientDataJSON: this.arrayBufferToBase64(authResponse.clientDataJSON),
          signature: this.arrayBufferToBase64(authResponse.signature),
          userHandle: authResponse.userHandle ? this.arrayBufferToBase64(authResponse.userHandle) : null,
        },
        type: credential.type,
      }),
    });

    if (!finishResponse.ok) {
      throw new Error('Authentication failed');
    }

    return finishResponse;
  }

  static async registerPasskey(name?: string): Promise<Response> {
    const startResponse = await fetch('/api/webauthn/register/start', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      credentials: 'same-origin',
      body: JSON.stringify({ name: name || 'Passkey' }),
    });

    if (!startResponse.ok) {
      if (startResponse.status === 401) {
        throw new Error('Your session has expired. Please sign in again.');
      }
      const errorText = await startResponse.text();
      throw new Error(`Failed to start passkey setup: ${errorText}`);
    }

    const response = await startResponse.json();
    const options = response.publicKey || response;
    const regId = response.reg_id;

    options.challenge = this.base64ToArrayBuffer(options.challenge);
    options.user.id = this.base64ToArrayBuffer(options.user.id);

    if (options.excludeCredentials) {
      options.excludeCredentials = options.excludeCredentials.map((cred: PublicKeyCredentialDescriptor) => ({
        ...cred,
        id: this.base64ToArrayBuffer(cred.id as unknown as string),
      }));
    }

    const credential = await navigator.credentials.create({
      publicKey: options,
    }) as PublicKeyCredential;

    if (!credential) {
      throw new Error('No credential created');
    }

    const attestationResponse = credential.response as AuthenticatorAttestationResponse;
    const finishResponse = await fetch(`/api/webauthn/register/finish/${regId}`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      credentials: 'same-origin',
      body: JSON.stringify({
        id: credential.id,
        rawId: this.arrayBufferToBase64(credential.rawId),
        response: {
          attestationObject: this.arrayBufferToBase64(attestationResponse.attestationObject),
          clientDataJSON: this.arrayBufferToBase64(attestationResponse.clientDataJSON),
        },
        type: credential.type,
      }),
    });

    if (!finishResponse.ok) {
      throw new Error('Failed to complete registration');
    }

    return finishResponse;
  }
}

export interface PasskeyInfo {
  id: string;
  name: string;
  created_at: number;
  last_used_at?: number;
}

export class PasskeyManager {
  static async loadPasskeys(): Promise<PasskeyInfo[]> {
    const response = await fetch('/api/webauthn/passkeys', {
      credentials: 'same-origin',
    });
    if (!response.ok) {
      throw new Error('Failed to load passkeys');
    }
    return await response.json();
  }

  static async deletePasskey(passkeyId: string): Promise<void> {
    const response = await fetch(`/api/webauthn/passkeys/${passkeyId}`, {
      method: 'DELETE',
      credentials: 'same-origin',
    });

    if (!response.ok) {
      throw new Error('Failed to delete passkey');
    }
  }

  static formatDate(timestamp: number): string {
    return new Date(timestamp * 1000).toLocaleDateString();
  }
}
