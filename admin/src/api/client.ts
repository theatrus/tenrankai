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
  can_manage_images: boolean;
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
  site_admins: string[];
  public_role: string | null;
  default_authenticated_role: string | null;
  roles: Record<string, RoleInfo>;
  user_roles: UserRoleAssignment[];
}

export interface GalleryInfo {
  name: string;
  url_prefix: string;
  permissions: PermissionConfig;
  image_count: number;
  total_size: number;
  total_size_formatted: string;
}

export interface GalleryListResponse {
  galleries: GalleryInfo[];
}

export type WatermarkPosition = 'bottom_left' | 'bottom_right' | 'top_left' | 'top_right' | 'center' | 'tiled';

export interface ImageWatermarkConfig {
  image: string;
  position: WatermarkPosition;
  opacity: number;
  scale: number;
  padding: number;
  adaptive: boolean;
  apply_to_gallery: boolean;
  apply_to_medium: boolean;
  apply_to_large: boolean;
}

export interface SiteGalleryInfo {
  name: string;
  url_prefix: string;
  source_directory: string;
  cache_directory: string;
  copyright_holder?: string;
  image_watermark?: ImageWatermarkConfig;
  enable_tile_zoom?: boolean;
}

export interface SiteGalleryListResponse {
  galleries: SiteGalleryInfo[];
}

export interface CreateGalleryRequest {
  name: string;
  url_prefix: string;
  source_directory: string;
  cache_directory: string;
  copyright_holder?: string;
  image_watermark?: ImageWatermarkConfig;
  enable_tile_zoom?: boolean;
}

export interface SiteInfo {
  name: string;
  hostnames: string[];
  base_url: string | null;
  templates: string[];
  static_files: string[];
  static_use_redirects: boolean;
  user_database: string | null;
  storage_prefix: string | null;
  cache_prefix: string | null;
  gallery_count: number;
  posts_count: number;
}

export interface SiteListResponse {
  sites: SiteInfo[];
}

export interface ReloadSiteResponse {
  success: boolean;
  message: string;
}

export interface FolderInfo {
  path: string;
  name: string;
  has_custom_permissions: boolean;
  image_count: number;
  size: number;
  size_formatted: string;
}

export interface FolderListResponse {
  folders: FolderInfo[];
}

export interface FolderPermissions {
  hidden: boolean;
  permissions: PermissionConfig;
  description: string;
}

export interface UpdateFolderPermissionsRequest {
  hidden: boolean;
  permissions: PermissionConfig;
  description: string;
}

export interface ShareFolderRequest {
  email: string;
  role: string;
}

export interface ShareFolderResponse {
  success: boolean;
  message: string;
  user_created: boolean;
}

export interface DeleteImagesRequest {
  paths: string[];
}

export interface DeleteImagesResponse {
  success: boolean;
  deleted_count: number;
  errors: string[];
}

export interface HideImagesRequest {
  paths: string[];
  hide: boolean;
}

export interface HideImagesResponse {
  success: boolean;
  hidden_images: string[];
}

export interface CreateFolderRequest {
  name: string;
  description?: string;
}

export interface CreateFolderResponse {
  success: boolean;
  folder_path: string;
}

export interface MoveImagesRequest {
  paths: string[];
  target_folder: string;
}

export interface MoveImagesResponse {
  success: boolean;
  moved_count: number;
  errors: string[];
}

export interface CopyImagesRequest {
  paths: string[];
  target_folder: string;
}

export interface CopyImagesResponse {
  success: boolean;
  copied_count: number;
  errors: string[];
}

export interface FolderImageInfo {
  url_id: string;
  filename: string;
  thumbnail_url: string;
  is_hidden: boolean;
}

export interface FolderImagesResponse {
  images: FolderImageInfo[];
}

export interface WatermarkImageInfo {
  filename: string;
  path: string;
}

export interface EnsureWatermarkFolderResponse {
  folder_path: string;
  created: boolean;
  images: WatermarkImageInfo[];
}

export interface ThemeColorSet {
  bg_primary?: string;
  bg_secondary?: string;
  bg_card?: string;
  bg_hover?: string;
  header_bg?: string;
  text_primary?: string;
  text_secondary?: string;
  text_muted?: string;
  link_color?: string;
  link_hover?: string;
  border_color?: string;
  accent_color?: string;
  btn_danger_bg?: string;
}

export interface GoogleFontConfig {
  family: string;
  weights: string[];
}

export interface ThemeConfig {
  force_color_scheme?: string;
  dark?: ThemeColorSet;
  light?: ThemeColorSet;
  font_body?: string;
  font_heading?: string;
  font_mono?: string;
  google_fonts?: GoogleFontConfig[];
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

  // Galleries (legacy - uses runtime gallery config)
  listGalleries: () => request<GalleryListResponse>('GET', '/galleries'),
  getGallery: (name: string) => request<GalleryInfo>('GET', `/galleries/${name}`),
  updateGalleryPermissions: (name: string, permissions: PermissionConfig) =>
    request<void>('PUT', `/galleries/${name}/permissions`, permissions),

  // Sites
  listSites: () => request<SiteListResponse>('GET', '/sites'),
  getSite: (name: string) => request<SiteInfo>('GET', `/sites/${name}`),
  reloadSite: (site: string) => request<ReloadSiteResponse>('POST', `/sites/${site}/reload`),

  // Site Galleries (ConfigStorage mode - editable)
  listSiteGalleries: (site: string) => request<SiteGalleryListResponse>('GET', `/sites/${site}/galleries`),
  getSiteGallery: (site: string, name: string) =>
    request<SiteGalleryInfo>('GET', `/sites/${site}/galleries/${name}`),
  createGallery: (site: string, name: string, data: CreateGalleryRequest) =>
    request<SiteGalleryInfo>('PUT', `/sites/${site}/galleries/${name}`, data),
  updateGallery: (site: string, name: string, data: CreateGalleryRequest) =>
    request<SiteGalleryInfo>('PUT', `/sites/${site}/galleries/${name}`, data),
  deleteGallery: (site: string, name: string) =>
    request<void>('DELETE', `/sites/${site}/galleries/${name}`),

  // Roles
  listRoles: () => request<RoleListResponse>('GET', '/roles'),
  getRole: (name: string) => request<RoleInfo>('GET', `/roles/${name}`),
  upsertRole: (name: string, role: Omit<RoleInfo, 'name' | 'is_builtin'>) =>
    request<void>('PUT', `/roles/${name}`, role),
  deleteRole: (name: string) => request<void>('DELETE', `/roles/${name}`),

  // Site Permissions
  getSitePermissions: (site: string) =>
    request<PermissionConfig>('GET', `/sites/${site}/permissions`),
  updateSitePermissions: (site: string, permissions: PermissionConfig) =>
    request<PermissionConfig>('PUT', `/sites/${site}/permissions`, permissions),

  // Gallery Folders
  listGalleryFolders: (site: string, gallery: string) =>
    request<FolderListResponse>('GET', `/sites/${site}/galleries/${gallery}/folders`),
  getFolderPermissions: (site: string, gallery: string, folderPath: string) =>
    request<FolderPermissions>('GET', `/sites/${site}/galleries/${gallery}/folders/${encodeURIComponent(folderPath || '_root')}`),
  updateFolderPermissions: (site: string, gallery: string, folderPath: string, data: UpdateFolderPermissionsRequest) =>
    request<FolderPermissions>('PUT', `/sites/${site}/galleries/${gallery}/folders/${encodeURIComponent(folderPath || '_root')}`, data),

  // Folder Sharing
  shareFolder: (site: string, gallery: string, folderPath: string, data: ShareFolderRequest) =>
    request<ShareFolderResponse>('POST', `/sites/${site}/galleries/${gallery}/folders/${encodeURIComponent(folderPath || '_root')}/share`, data),

  // Folder Images
  listFolderImages: (site: string, gallery: string, folderPath: string) =>
    request<FolderImagesResponse>('GET', `/sites/${site}/galleries/${gallery}/folders/${encodeURIComponent(folderPath || '_root')}/images`),

  // Image Management
  deleteGalleryImages: (site: string, gallery: string, paths: string[]) =>
    request<DeleteImagesResponse>('DELETE', `/sites/${site}/galleries/${gallery}/images`, { paths }),

  // Image Management (resolved - site determined from host)
  hideImages: (gallery: string, folderPath: string, data: HideImagesRequest) =>
    request<HideImagesResponse>('POST', `/galleries/${gallery}/folders/${encodeURIComponent(folderPath || '_root')}/images/hide`, data),

  moveImages: (gallery: string, folderPath: string, data: MoveImagesRequest) =>
    request<MoveImagesResponse>('POST', `/galleries/${gallery}/folders/${encodeURIComponent(folderPath || '_root')}/images/move`, data),

  copyImages: (gallery: string, folderPath: string, data: CopyImagesRequest) =>
    request<CopyImagesResponse>('POST', `/galleries/${gallery}/folders/${encodeURIComponent(folderPath || '_root')}/images/copy`, data),

  // Folder Management (resolved - site determined from host)
  createFolder: (gallery: string, parentPath: string, data: CreateFolderRequest) =>
    request<CreateFolderResponse>('POST', `/galleries/${gallery}/folders/${encodeURIComponent(parentPath || '_root')}/create`, data),

  deleteFolder: (gallery: string, folderPath: string) =>
    request<{ success: boolean; message: string }>('DELETE', `/galleries/${gallery}/folders/${encodeURIComponent(folderPath || '_root')}`),

  // Watermark Folder
  ensureWatermarkFolder: (gallery: string) =>
    request<EnsureWatermarkFolderResponse>('POST', `/galleries/${gallery}/watermark-folder`),

  // Theme Management
  getTheme: () => request<ThemeConfig>('GET', '/theme'),
  updateTheme: (theme: ThemeConfig) => request<ThemeConfig>('PUT', '/theme', theme),
  resetTheme: () => request<void>('DELETE', '/theme'),
};
