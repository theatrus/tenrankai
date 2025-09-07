import { PreviewResponse, AuthCredentials, ApiError, LoginResponse, LoginCredentials } from './types.js';

export class ApiClient {
  private static baseUrl = '';

  static async getGalleryPreview(
    galleryName: string, 
    count: number = 6
  ): Promise<PreviewResponse> {
    try {
      const response = await fetch(
        `/api/gallery/${galleryName}/preview?count=${count}`
      );
      
      if (!response.ok) {
        throw this.createApiError(response);
      }
      
      return await response.json();
    } catch (error) {
      throw this.handleFetchError(error);
    }
  }

  static async authenticateWithPasskey(
    username: string,
    credential: AuthCredentials  
  ): Promise<LoginResponse> {
    try {
      const response = await fetch('/_login/webauthn/authenticate', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ username, credential })
      });
      
      if (!response.ok) {
        throw this.createApiError(response);  
      }
      
      return await response.json();
    } catch (error) {
      throw this.handleFetchError(error);
    }
  }

  static async requestEmailLogin(credentials: LoginCredentials): Promise<LoginResponse> {
    try {
      const response = await fetch('/_login/request', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(credentials)
      });
      
      if (!response.ok) {
        throw this.createApiError(response);
      }
      
      return await response.json();
    } catch (error) {
      throw this.handleFetchError(error);
    }
  }

  static async checkAuthStatus(): Promise<{ authenticated: boolean; username?: string }> {
    try {
      const response = await fetch('/api/verify');
      
      if (!response.ok) {
        return { authenticated: false };
      }
      
      return await response.json();
    } catch (error) {
      return { authenticated: false };
    }
  }

  private static createApiError(response: Response): ApiError {
    return {
      message: `API request failed: ${response.statusText}`,
      status: response.status,
      type: response.status >= 500 ? 'server' : 'client'
    };
  }

  private static handleFetchError(error: unknown): ApiError {
    if (error instanceof TypeError) {
      return {
        message: 'Network error occurred',
        status: 0,
        type: 'network'
      };
    }
    throw error;
  }
}