// WebAuthn utility functions extracted from login.js

export class WebAuthnUtils {
  /**
   * Convert base64 to ArrayBuffer (handles URL-safe base64)
   */
  static base64ToArrayBuffer(base64: string): ArrayBuffer {
    // Handle URL-safe base64 (convert to standard base64)
    const standardBase64 = base64.replace(/-/g, '+').replace(/_/g, '/');
    // Add padding if necessary
    const padding = standardBase64.length % 4;
    const paddedBase64 = padding ? standardBase64 + '===='.substring(padding) : standardBase64;
    
    const binaryString = window.atob(paddedBase64);
    const bytes = new Uint8Array(binaryString.length);
    for (let i = 0; i < binaryString.length; i++) {
        bytes[i] = binaryString.charCodeAt(i);
    }
    return bytes.buffer;
  }

  /**
   * Convert ArrayBuffer to URL-safe base64
   */
  static arrayBufferToBase64(buffer: ArrayBuffer): string {
    const bytes = new Uint8Array(buffer);
    let binary = '';
    for (let i = 0; i < bytes.byteLength; i++) {
        binary += String.fromCharCode(bytes[i]);
    }
    // Return URL-safe base64 to match what the server expects
    return window.btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=/g, '');
  }

  /**
   * Check if WebAuthn is supported
   */
  static isSupported(): boolean {
    return !!window.PublicKeyCredential;
  }

  /**
   * Check if user has passkeys
   */
  static async checkUserHasPasskeys(username: string): Promise<{ has_passkeys: boolean }> {
    const response = await fetch('/api/webauthn/check-passkeys', {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({ username })
    });
    
    if (response.ok) {
        return await response.json();
    }
    
    return { has_passkeys: false };
  }

  /**
   * Start passkey authentication flow
   */
  static async authenticateWithPasskey(username: string): Promise<Response> {
    // Start passkey authentication
    const startResponse = await fetch('/api/webauthn/authenticate/start', {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({ username })
    });
    
    if (!startResponse.ok) {
        throw new Error('Failed to start authentication');
    }
    
    const response = await startResponse.json();
    
    // Extract the publicKey options and auth_id
    const options = response.publicKey || response;
    const authId = response.auth_id;
    
    // Convert challenge from base64
    options.challenge = this.base64ToArrayBuffer(options.challenge);
    
    // Convert allowCredentials
    if (options.allowCredentials) {
        options.allowCredentials = options.allowCredentials.map((cred: any) => ({
            ...cred,
            id: this.base64ToArrayBuffer(cred.id)
        }));
    }
    
    // Get credential
    const credential = await navigator.credentials.get({
        publicKey: options
    }) as PublicKeyCredential;
    
    if (!credential) {
        throw new Error('No credential obtained');
    }

    const authResponse = credential.response as AuthenticatorAssertionResponse;
    
    // Send authentication response
    const finishResponse = await fetch(`/api/webauthn/authenticate/finish/${authId}`, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({
            id: credential.id,
            rawId: this.arrayBufferToBase64(credential.rawId),
            response: {
                authenticatorData: this.arrayBufferToBase64(authResponse.authenticatorData),
                clientDataJSON: this.arrayBufferToBase64(authResponse.clientDataJSON),
                signature: this.arrayBufferToBase64(authResponse.signature),
                userHandle: authResponse.userHandle ? this.arrayBufferToBase64(authResponse.userHandle) : null
            },
            type: credential.type
        })
    });
    
    if (!finishResponse.ok) {
        throw new Error('Authentication failed');
    }
    
    return finishResponse;
  }

  /**
   * Register a new passkey
   */
  static async registerPasskey(name?: string): Promise<Response> {
    // Start registration
    const startResponse = await fetch('/api/webauthn/register/start', {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({ name: name || 'Passkey' })
    });
    
    if (!startResponse.ok) {
        if (startResponse.status === 401) {
            throw new Error('Your session has expired. Please sign in again.');
        }
        const errorText = await startResponse.text();
        throw new Error('Failed to start passkey setup: ' + errorText);
    }
    
    const response = await startResponse.json();
    
    // Extract the publicKey options and reg_id
    const options = response.publicKey || response;
    const regId = response.reg_id;
    
    // Convert challenge and user.id from base64
    options.challenge = this.base64ToArrayBuffer(options.challenge);
    options.user.id = this.base64ToArrayBuffer(options.user.id);
    
    // Convert excludeCredentials if present
    if (options.excludeCredentials) {
        options.excludeCredentials = options.excludeCredentials.map((cred: any) => ({
            ...cred,
            id: this.base64ToArrayBuffer(cred.id)
        }));
    }
    
    // Create credential
    const credential = await navigator.credentials.create({
        publicKey: options
    }) as PublicKeyCredential;
    
    if (!credential) {
        throw new Error('No credential created');
    }

    const attestationResponse = credential.response as AuthenticatorAttestationResponse;
    
    // Send registration response
    const finishResponse = await fetch(`/api/webauthn/register/finish/${regId}`, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({
            id: credential.id,
            rawId: this.arrayBufferToBase64(credential.rawId),
            response: {
                attestationObject: this.arrayBufferToBase64(attestationResponse.attestationObject),
                clientDataJSON: this.arrayBufferToBase64(attestationResponse.clientDataJSON)
            },
            type: credential.type
        })
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
}

export class PasskeyManager {
  /**
   * Load user's passkeys
   */
  static async loadPasskeys(): Promise<PasskeyInfo[]> {
    const response = await fetch('/api/webauthn/passkeys');
    if (!response.ok) {
        throw new Error('Failed to load passkeys');
    }
    return await response.json();
  }

  /**
   * Delete a passkey
   */
  static async deletePasskey(passkeyId: string): Promise<void> {
    const response = await fetch(`/api/webauthn/passkeys/${passkeyId}`, {
        method: 'DELETE'
    });
    
    if (!response.ok) {
        throw new Error('Failed to delete passkey');
    }
  }

  /**
   * Format timestamp for display
   */
  static formatDate(timestamp: number): string {
    return new Date(timestamp * 1000).toLocaleDateString();
  }
}