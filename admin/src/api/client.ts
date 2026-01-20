const API_BASE = '/_admin/api';

export interface UserInfo {
  username: string;
  email: string;
  passkey_count: number;
}

export interface UserListResponse {
  users: UserInfo[];
}

export interface CreateUserRequest {
  username: string;
  email: string;
  send_invite: boolean;
}

export interface UpdateUserRequest {
  email?: string;
}

export interface RolePermissions {
  can_view: boolean;
  can_see_technical_details: boolean;
  can_see_exact_dates: boolean;
  can_see_location: boolean;
  can_download_medium: boolean;
  can_download_large: boolean;
  can_download_original: boolean;
  can_download_gallery: boolean;
  can_download_raw: boolean;
  can_see_versions: boolean;
  can_read_metadata: boolean;
  can_edit_content: boolean;
  can_add_comments: boolean;
  can_edit_own_comments: boolean;
  can_delete_own_comments: boolean;
  can_edit_any_comments: boolean;
  can_delete_any_comments: boolean;
  can_set_picks: boolean;
  can_add_tags: boolean;
  can_use_zoom: boolean;
  can_use_tile_zoom: boolean;
  can_analyze_images: boolean;
  can_see_ai_analysis: boolean;
  can_see_ai_alt_text: boolean;
  owner_access: boolean;
}

export interface RoleInfo {
  name: string;
  permissions: RolePermissions;
  inherits: string | null;
  is_builtin: boolean;
}

export interface RoleListResponse {
  roles: RoleInfo[];
}

export interface UserRoleAssignment {
  username: string;
  roles: string[];
}

export interface PermissionConfig {
  public_role: string | null;
  default_authenticated_role: string | null;
  roles: Record<string, RoleInfo>;
  user_roles: UserRoleAssignment[];
}

export interface GalleryInfo {
  name: string;
  url_prefix: string;
  permissions: PermissionConfig;
}

export interface GalleryListResponse {
  galleries: GalleryInfo[];
}

class ApiError extends Error {
  constructor(
    message: string,
    public status: number
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, {
    method,
    headers: {
      'Content-Type': 'application/json',
    },
    credentials: 'same-origin',
    body: body ? JSON.stringify(body) : undefined,
  });

  if (!response.ok) {
    if (response.status === 401) {
      window.location.href = `/_login?return=${encodeURIComponent(window.location.pathname)}`;
      throw new ApiError('Unauthorized', 401);
    }
    if (response.status === 403) {
      throw new ApiError('Access denied. Owner permission required.', 403);
    }
    const text = await response.text();
    throw new ApiError(text || `HTTP ${response.status}`, response.status);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return response.json();
}

export const api = {
  // Users
  listUsers: () => request<UserListResponse>('GET', '/users'),
  getUser: (username: string) => request<UserInfo>('GET', `/users/${username}`),
  createUser: (data: CreateUserRequest) => request<UserInfo>('POST', '/users', data),
  updateUser: (username: string, data: UpdateUserRequest) =>
    request<UserInfo>('PUT', `/users/${username}`, data),
  deleteUser: (username: string) => request<void>('DELETE', `/users/${username}`),
  sendInvite: (username: string) => request<void>('POST', `/users/${username}/invite`),

  // Galleries
  listGalleries: () => request<GalleryListResponse>('GET', '/galleries'),
  getGallery: (name: string) => request<GalleryInfo>('GET', `/galleries/${name}`),
  updateGalleryPermissions: (name: string, permissions: PermissionConfig) =>
    request<void>('PUT', `/galleries/${name}/permissions`, permissions),

  // Roles
  listRoles: () => request<RoleListResponse>('GET', '/roles'),
  getRole: (name: string) => request<RoleInfo>('GET', `/roles/${name}`),
  upsertRole: (name: string, role: Omit<RoleInfo, 'name' | 'is_builtin'>) =>
    request<void>('PUT', `/roles/${name}`, role),
  deleteRole: (name: string) => request<void>('DELETE', `/roles/${name}`),
};
