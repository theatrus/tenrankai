// Login and WebAuthn utility functions

// Base64 conversion utilities
function base64ToArrayBuffer(base64) {
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

function arrayBufferToBase64(buffer) {
    const bytes = new Uint8Array(buffer);
    let binary = '';
    for (let i = 0; i < bytes.byteLength; i++) {
        binary += String.fromCharCode(bytes[i]);
    }
    // Return URL-safe base64 to match what the server expects
    return window.btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=/g, '');
}

// HTML escaping for security
function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

// Cookie utilities
function getCookie(name) {
    const cookies = document.cookie.split(';');
    for (let cookie of cookies) {
        const [cookieName, cookieValue] = cookie.trim().split('=');
        if (cookieName === name && cookieValue) {
            return decodeURIComponent(cookieValue);
        }
    }
    return null;
}

function getReturnUrl() {
    return getCookie('return_url');
}

// WebAuthn support detection
function isWebAuthnSupported() {
    return !!window.PublicKeyCredential;
}

// Passkey authentication flow
async function authenticateWithPasskey(username) {
    // Start passkey authentication
    const startResponse = await fetch('/api/webauthn/authenticate/start', {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({ username: username })
    });
    
    if (!startResponse.ok) {
        throw new Error('Failed to start authentication');
    }
    
    const response = await startResponse.json();
    
    // Extract the publicKey options and auth_id
    const options = response.publicKey || response;
    const authId = response.auth_id;
    
    // Convert challenge from base64
    options.challenge = base64ToArrayBuffer(options.challenge);
    
    // Convert allowCredentials
    if (options.allowCredentials) {
        options.allowCredentials = options.allowCredentials.map(cred => ({
            ...cred,
            id: base64ToArrayBuffer(cred.id)
        }));
    }
    
    // Get credential
    const credential = await navigator.credentials.get({
        publicKey: options
    });
    
    // Send authentication response
    const finishResponse = await fetch(`/api/webauthn/authenticate/finish/${authId}`, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({
            id: credential.id,
            rawId: arrayBufferToBase64(credential.rawId),
            response: {
                authenticatorData: arrayBufferToBase64(credential.response.authenticatorData),
                clientDataJSON: arrayBufferToBase64(credential.response.clientDataJSON),
                signature: arrayBufferToBase64(credential.response.signature),
                userHandle: credential.response.userHandle ? arrayBufferToBase64(credential.response.userHandle) : null
            },
            type: credential.type
        })
    });
    
    if (!finishResponse.ok) {
        throw new Error('Authentication failed');
    }
    
    return finishResponse;
}

// Passkey registration flow
async function registerPasskey(name) {
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
    options.challenge = base64ToArrayBuffer(options.challenge);
    options.user.id = base64ToArrayBuffer(options.user.id);
    
    // Convert excludeCredentials if present
    if (options.excludeCredentials) {
        options.excludeCredentials = options.excludeCredentials.map(cred => ({
            ...cred,
            id: base64ToArrayBuffer(cred.id)
        }));
    }
    
    // Create credential
    const credential = await navigator.credentials.create({
        publicKey: options
    });
    
    // Send registration response
    const finishResponse = await fetch(`/api/webauthn/register/finish/${regId}`, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({
            id: credential.id,
            rawId: arrayBufferToBase64(credential.rawId),
            response: {
                attestationObject: arrayBufferToBase64(credential.response.attestationObject),
                clientDataJSON: arrayBufferToBase64(credential.response.clientDataJSON)
            },
            type: credential.type
        })
    });
    
    if (!finishResponse.ok) {
        throw new Error('Failed to complete registration');
    }
    
    return finishResponse;
}

// Check if user has passkeys
async function checkUserHasPasskeys(username) {
    const response = await fetch('/api/webauthn/check-passkeys', {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({ username: username })
    });
    
    if (response.ok) {
        return await response.json();
    }
    
    return { has_passkeys: false };
}

// Show/hide elements utility
function showElement(elementId) {
    const element = document.getElementById(elementId);
    if (element) {
        element.style.display = 'block';
    }
}

function hideElement(elementId) {
    const element = document.getElementById(elementId);
    if (element) {
        element.style.display = 'none';
    }
}

// Error message display utility
function showError(message, elementId = 'errorMessage') {
    const errorDiv = document.getElementById(elementId);
    if (errorDiv) {
        errorDiv.textContent = message;
        errorDiv.style.display = 'block';
    }
}

function hideError(elementId = 'errorMessage') {
    const errorDiv = document.getElementById(elementId);
    if (errorDiv) {
        errorDiv.style.display = 'none';
    }
}

// Success message display utility
function showSuccess(message, elementId = 'successMessage') {
    const successDiv = document.getElementById(elementId);
    if (successDiv) {
        successDiv.textContent = message;
        successDiv.style.display = 'block';
    }
}

function hideSuccess(elementId = 'successMessage') {
    const successDiv = document.getElementById(elementId);
    if (successDiv) {
        successDiv.style.display = 'none';
    }
}

// Passkey list management
async function loadPasskeys() {
    const response = await fetch('/api/webauthn/passkeys');
    if (!response.ok) {
        throw new Error('Failed to load passkeys');
    }
    return await response.json();
}

async function deletePasskey(passkeyId) {
    const response = await fetch(`/api/webauthn/passkeys/${passkeyId}`, {
        method: 'DELETE'
    });
    
    if (!response.ok) {
        throw new Error('Failed to delete passkey');
    }
    
    return response;
}

// Format date utility
function formatDate(timestamp) {
    return new Date(timestamp * 1000).toLocaleDateString();
}

// Profile page passkey management functions
async function loadPasskeyList() {
    const passkeyList = document.getElementById('passkeyList');
    if (!passkeyList) return;
    
    try {
        const passkeys = await loadPasskeys();
        
        if (passkeys.length === 0) {
            passkeyList.innerHTML = '<div class="loading">No passkeys found</div>';
            return;
        }
        
        passkeyList.innerHTML = passkeys.map(passkey => `
            <div class="passkey-item" data-id="${passkey.id}">
                <div class="passkey-info">
                    <div class="passkey-name">${escapeHtml(passkey.name || 'Unnamed Passkey')}</div>
                    <div class="passkey-created">Created: ${formatDate(passkey.created_at)}</div>
                </div>
                <div class="passkey-actions">
                    <button type="button" class="btn-danger" onclick="LoginUtils.removePasskey('${passkey.id}')">Remove</button>
                </div>
            </div>
        `).join('');
        
    } catch (error) {
        console.error('Error loading passkeys:', error);
        passkeyList.innerHTML = '<div class="error">Failed to load passkeys</div>';
    }
}

async function removePasskey(passkeyId) {
    if (!confirm('Are you sure you want to remove this passkey? This action cannot be undone.')) {
        return;
    }
    
    try {
        await deletePasskey(passkeyId);
        
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

async function enrollNewPasskey() {
    try {
        // Redirect to enrollment page
        window.location.href = '/_login/passkey-enrollment?return=' + encodeURIComponent('/_login/profile');
        
    } catch (error) {
        console.error('Error starting passkey enrollment:', error);
        alert('Failed to start passkey enrollment. Please try again.');
    }
}

// Initialize profile page functionality
function initProfilePage() {
    // Load passkeys if the passkey list element exists
    if (document.getElementById('passkeyList')) {
        loadPasskeyList();
    }
}

// Export functions for use in templates
window.LoginUtils = {
    base64ToArrayBuffer,
    arrayBufferToBase64,
    escapeHtml,
    getCookie,
    getReturnUrl,
    isWebAuthnSupported,
    authenticateWithPasskey,
    registerPasskey,
    checkUserHasPasskeys,
    showElement,
    hideElement,
    showError,
    hideError,
    showSuccess,
    hideSuccess,
    loadPasskeys,
    deletePasskey,
    formatDate,
    loadPasskeyList,
    removePasskey,
    enrollNewPasskey,
    initProfilePage
};