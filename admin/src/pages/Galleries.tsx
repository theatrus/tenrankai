import { useState, useEffect, useCallback } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Link, useParams } from 'react-router-dom';
import {
  api,
  GalleryInfo,
  SiteGalleryInfo,
  CreateGalleryRequest,
  PermissionConfig,
  RoleInfo,
  FolderInfo,
  ImageWatermarkConfig,
  WatermarkPosition,
  WatermarkImageInfo,
} from '@api/client';
import { useHostedMode } from '@hooks/useHostedMode';

const DEFAULT_SITE = 'default';

export function Galleries() {
  const { name, '*': folderPath } = useParams<{ name: string; '*': string }>();
  const queryClient = useQueryClient();
  const hostedMode = useHostedMode();
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [newGallery, setNewGallery] = useState<CreateGalleryRequest>({
    name: '',
    url_prefix: '/gallery',
    source_directory: '',
    cache_directory: '',
  });

  // Check if we have ConfigStorage mode by fetching sites
  const { data: sitesData } = useQuery({
    queryKey: ['sites'],
    queryFn: api.listSites,
  });

  const hasConfigStorage = sitesData && sitesData.sites.length > 0;

  // Fetch runtime galleries (always available)
  const { data: runtimeData, isLoading: runtimeLoading, error: runtimeError } = useQuery({
    queryKey: ['galleries'],
    queryFn: api.listGalleries,
  });

  // Fetch site galleries (ConfigStorage mode)
  const { data: siteGalleriesData, isLoading: siteLoading } = useQuery({
    queryKey: ['siteGalleries', DEFAULT_SITE],
    queryFn: () => api.listSiteGalleries(DEFAULT_SITE),
    enabled: hasConfigStorage,
  });

  const createMutation = useMutation({
    mutationFn: (data: CreateGalleryRequest) => api.createGallery(DEFAULT_SITE, data.name, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['siteGalleries'] });
      queryClient.invalidateQueries({ queryKey: ['galleries'] });
      setShowCreateModal(false);
      setNewGallery({
        name: '',
        url_prefix: '/gallery',
        source_directory: '',
        cache_directory: '',
      });
    },
  });


  const deleteMutation = useMutation({
    mutationFn: (galleryName: string) => api.deleteGallery(DEFAULT_SITE, galleryName),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['siteGalleries'] });
      queryClient.invalidateQueries({ queryKey: ['galleries'] });
    },
  });

  const reloadMutation = useMutation({
    mutationFn: () => api.reloadSite(DEFAULT_SITE),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['galleries'] });
    },
  });

  const handleCreate = (e: React.FormEvent) => {
    e.preventDefault();
    createMutation.mutate(newGallery);
  };

  const handleDelete = (galleryName: string) => {
    if (confirm(`Are you sure you want to delete gallery "${galleryName}"? This cannot be undone.`)) {
      deleteMutation.mutate(galleryName);
    }
  };

  const handleReload = () => {
    reloadMutation.mutate();
  };

  if (runtimeError) {
    return <div className="error">Failed to load galleries: {String(runtimeError)}</div>;
  }

  if (name) {
    return <GalleryDetail name={name} initialFolderPath={folderPath} hostedMode={hostedMode} />;
  }

  const isLoading = runtimeLoading || (hasConfigStorage && siteLoading);

  return (
    <div>
      <header className="page-header">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <div>
            <h1 className="page-title">Galleries</h1>
            <p className="page-subtitle">Manage gallery settings and permissions</p>
          </div>
          <div style={{ display: 'flex', gap: '0.5rem' }}>
            {hasConfigStorage && (
              <>
                <button
                  className="btn btn-secondary"
                  onClick={handleReload}
                  disabled={reloadMutation.isPending}
                >
                  {reloadMutation.isPending ? 'Reloading...' : 'Reload Site'}
                </button>
                <button className="btn btn-primary" onClick={() => setShowCreateModal(true)}>
                  Add Gallery
                </button>
              </>
            )}
          </div>
        </div>
      </header>

      {reloadMutation.isSuccess && (
        <div className="card" style={{ background: 'var(--color-success)', color: 'white', marginBottom: '1rem' }}>
          {reloadMutation.data?.message || 'Site reloaded successfully'}
        </div>
      )}

      {!hasConfigStorage && (
        <div className="card" style={{ marginBottom: '1rem', background: 'var(--color-warning-bg)' }}>
          <p style={{ margin: 0 }}>
            <strong>ConfigStorage not available:</strong> No sites found.
            Ensure <code>config_storage</code> is set in config.toml and run <code>cargo run -- config init</code> to initialize.
          </p>
        </div>
      )}

      <div className="card">
        {isLoading ? (
          <div className="loading">Loading galleries...</div>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Name</th>
                <th>URL Prefix</th>
                {hasConfigStorage && !hostedMode && <th>Source Directory</th>}
                <th>Images</th>
                <th>Size</th>
                <th>Public Role</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {runtimeData?.galleries.map((gallery: GalleryInfo) => {
                const siteGallery = siteGalleriesData?.galleries.find(g => g.name === gallery.name);
                return (
                  <tr key={gallery.name}>
                    <td>{gallery.name}</td>
                    <td>
                      <code>{gallery.url_prefix}</code>
                    </td>
                    {hasConfigStorage && !hostedMode && (
                      <td>
                        <code>{siteGallery?.source_directory || '-'}</code>
                      </td>
                    )}
                    <td>{gallery.image_count.toLocaleString()}</td>
                    <td>{gallery.total_size_formatted}</td>
                    <td>
                      <span className={`badge ${gallery.permissions.public_role ? 'badge-success' : 'badge-warning'}`}>
                        {gallery.permissions.public_role || 'none (private)'}
                      </span>
                    </td>
                    <td>
                      <div className="actions">
                        <Link to={`/galleries/${gallery.name}`} className="btn btn-secondary">
                          Manage
                        </Link>
                        {hasConfigStorage && siteGallery && (
                          <button
                            className="btn btn-danger"
                            onClick={() => handleDelete(gallery.name)}
                            disabled={deleteMutation.isPending}
                          >
                            Delete
                          </button>
                        )}
                      </div>
                    </td>
                  </tr>
                );
              })}
              {runtimeData?.galleries.length === 0 && (
                <tr>
                  <td colSpan={hasConfigStorage ? 7 : 6} style={{ textAlign: 'center', color: 'var(--color-text-muted)' }}>
                    No galleries configured
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        )}
      </div>

      {/* Create Gallery Modal */}
      {showCreateModal && (
        <div className="modal-overlay" onClick={() => setShowCreateModal(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">Create Gallery</div>
            <form onSubmit={handleCreate}>
              <div className="form-group">
                <label className="form-label">Name</label>
                <input
                  type="text"
                  className="form-input"
                  value={newGallery.name}
                  onChange={(e) => setNewGallery({ ...newGallery, name: e.target.value })}
                  required
                  pattern="[a-zA-Z0-9_-]+"
                  title="Alphanumeric characters, underscores, and hyphens only"
                  placeholder="main"
                />
              </div>
              <div className="form-group">
                <label className="form-label">URL Prefix</label>
                <input
                  type="text"
                  className="form-input"
                  value={newGallery.url_prefix}
                  onChange={(e) => setNewGallery({ ...newGallery, url_prefix: e.target.value })}
                  required
                  pattern="/[a-zA-Z0-9/_-]*"
                  title="Must start with / (e.g., /gallery, /photos)"
                  placeholder="/gallery"
                />
              </div>
              {!hostedMode && (
                <>
                  <div className="form-group">
                    <label className="form-label">Source Directory</label>
                    <input
                      type="text"
                      className="form-input"
                      value={newGallery.source_directory}
                      onChange={(e) => setNewGallery({ ...newGallery, source_directory: e.target.value })}
                      required
                      placeholder="photos"
                    />
                    <small style={{ color: 'var(--color-text-muted)' }}>
                      Relative to the site's storage prefix
                    </small>
                  </div>
                  <div className="form-group">
                    <label className="form-label">Cache Directory</label>
                    <input
                      type="text"
                      className="form-input"
                      value={newGallery.cache_directory}
                      onChange={(e) => setNewGallery({ ...newGallery, cache_directory: e.target.value })}
                      required
                      placeholder="cache/main"
                    />
                    <small style={{ color: 'var(--color-text-muted)' }}>
                      Relative to the site's storage prefix
                    </small>
                  </div>
                </>
              )}
              <div style={{ borderTop: '1px solid var(--color-border)', marginTop: '0.5rem', paddingTop: '0.75rem' }}>
                <h4 style={{ margin: '0 0 0.5rem 0', fontSize: '0.9rem' }}>Layout</h4>
                <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '0.75rem' }}>
                  <div className="form-group">
                    <label className="form-label">Grid Mode</label>
                    <select
                      className="form-input"
                      value={newGallery.grid_mode || 'masonry'}
                      onChange={(e) => setNewGallery({ ...newGallery, grid_mode: e.target.value })}
                    >
                      <option value="masonry">Masonry</option>
                      <option value="square">Square</option>
                    </select>
                  </div>
                  <div className="form-group">
                    <label className="form-label">Max Columns</label>
                    <input
                      type="number"
                      className="form-input"
                      min="1"
                      max="5"
                      value={newGallery.max_columns ?? ''}
                      onChange={(e) => setNewGallery({ ...newGallery, max_columns: e.target.value ? Number(e.target.value) : undefined })}
                      placeholder="Default (2)"
                    />
                  </div>
                </div>
              </div>
              {createMutation.error && (
                <div className="error" style={{ marginBottom: '1rem' }}>
                  {String(createMutation.error)}
                </div>
              )}
              <div className="modal-actions">
                <button type="button" className="btn btn-secondary" onClick={() => setShowCreateModal(false)}>
                  Cancel
                </button>
                <button type="submit" className="btn btn-primary" disabled={createMutation.isPending}>
                  {createMutation.isPending ? 'Creating...' : 'Create Gallery'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

    </div>
  );
}

const WATERMARK_POSITIONS: { value: WatermarkPosition; label: string }[] = [
  { value: 'bottom_right', label: 'Bottom Right' },
  { value: 'bottom_left', label: 'Bottom Left' },
  { value: 'top_right', label: 'Top Right' },
  { value: 'top_left', label: 'Top Left' },
  { value: 'center', label: 'Center' },
  { value: 'tiled', label: 'Tiled' },
];

const BUILTIN_ROLES = ['viewer', 'contributor', 'admin'];

function getAvailableRoles(customRoles: Record<string, RoleInfo>): string[] {
  const roleSet = new Set(BUILTIN_ROLES);
  Object.keys(customRoles).forEach((r) => roleSet.add(r));
  return Array.from(roleSet);
}

function GalleryDetail({ name, initialFolderPath, hostedMode }: { name: string; initialFolderPath?: string; hostedMode: boolean }) {
  const queryClient = useQueryClient();
  const [showAddUser, setShowAddUser] = useState(false);
  const [newUserAssignment, setNewUserAssignment] = useState({ username: '', roles: ['viewer'] });
  const [localPermissions, setLocalPermissions] = useState<PermissionConfig | null>(null);
  const [hasChanges, setHasChanges] = useState(false);
  const [editFolder, setEditFolder] = useState<FolderInfo | null>(null);
  const [initialFolderOpened, setInitialFolderOpened] = useState(false);
  const [showCreateFolder, setShowCreateFolder] = useState(false);
  const [createFolderParent, setCreateFolderParent] = useState<string>('');

  // Gallery settings state
  const [gallerySettings, setGallerySettings] = useState<SiteGalleryInfo | null>(null);
  const [hasSettingsChanges, setHasSettingsChanges] = useState(false);
  const [showWatermarkSettings, setShowWatermarkSettings] = useState(false);
  const [enableTextWatermark, setEnableTextWatermark] = useState(false);
  const [enableImageWatermark, setEnableImageWatermark] = useState(false);
  const [enableTileZoom, setEnableTileZoom] = useState(false);
  const [watermarkImages, setWatermarkImages] = useState<WatermarkImageInfo[]>([]);
  const [watermarkFolderLoading, setWatermarkFolderLoading] = useState(false);

  const { data, isLoading, error } = useQuery({
    queryKey: ['gallery', name],
    queryFn: () => api.getGallery(name),
  });

  const { data: sitePermissions } = useQuery({
    queryKey: ['sitePermissions', DEFAULT_SITE],
    queryFn: () => api.getSitePermissions(DEFAULT_SITE),
  });

  // Fetch site gallery config for editing
  const { data: siteGalleryData } = useQuery({
    queryKey: ['siteGallery', DEFAULT_SITE, name],
    queryFn: () => api.getSiteGallery(DEFAULT_SITE, name),
  });

  // Initialize gallery settings when data loads
  useEffect(() => {
    if (siteGalleryData && !gallerySettings) {
      setGallerySettings(siteGalleryData);
      setShowWatermarkSettings(!!(siteGalleryData.copyright_holder || siteGalleryData.image_watermark));
      setEnableTextWatermark(!!siteGalleryData.copyright_holder);
      setEnableImageWatermark(!!siteGalleryData.image_watermark);
      setEnableTileZoom(!!siteGalleryData.enable_tile_zoom);
    }
  }, [siteGalleryData, gallerySettings]);

  const updateGalleryMutation = useMutation({
    mutationFn: (data: CreateGalleryRequest) => api.updateGallery(DEFAULT_SITE, data.name, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['siteGallery'] });
      queryClient.invalidateQueries({ queryKey: ['galleries'] });
      setHasSettingsChanges(false);
    },
  });

  const { data: usersData } = useQuery({
    queryKey: ['users'],
    queryFn: api.listUsers,
  });

  const { data: foldersData } = useQuery({
    queryKey: ['galleryFolders', DEFAULT_SITE, name],
    queryFn: () => api.listGalleryFolders(DEFAULT_SITE, name),
  });

  const updatePermissionsMutation = useMutation({
    mutationFn: (permissions: PermissionConfig) => api.updateSitePermissions(DEFAULT_SITE, permissions),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['sitePermissions'] });
      queryClient.invalidateQueries({ queryKey: ['gallery'] });
      setHasChanges(false);
    },
  });

  const updateGallerySettings = useCallback((updates: Partial<SiteGalleryInfo>) => {
    if (!gallerySettings) return;
    setGallerySettings({ ...gallerySettings, ...updates });
    setHasSettingsChanges(true);
  }, [gallerySettings]);

  const updateWatermark = useCallback((updates: Partial<ImageWatermarkConfig>) => {
    if (!gallerySettings) return;
    setGallerySettings({
      ...gallerySettings,
      image_watermark: {
        image: gallerySettings.image_watermark?.image || '',
        position: gallerySettings.image_watermark?.position || 'bottom_right',
        opacity: gallerySettings.image_watermark?.opacity ?? 0.5,
        scale: gallerySettings.image_watermark?.scale ?? 15,
        padding: gallerySettings.image_watermark?.padding ?? 10,
        adaptive: gallerySettings.image_watermark?.adaptive ?? true,
        apply_to_gallery: gallerySettings.image_watermark?.apply_to_gallery ?? false,
        apply_to_medium: gallerySettings.image_watermark?.apply_to_medium ?? true,
        apply_to_large: gallerySettings.image_watermark?.apply_to_large ?? false,
        ...updates,
      },
    });
    setHasSettingsChanges(true);
  }, [gallerySettings]);

  const handleSaveSettings = () => {
    if (!gallerySettings) return;
    const request: CreateGalleryRequest = {
      name: gallerySettings.name,
      url_prefix: gallerySettings.url_prefix,
      source_directory: gallerySettings.source_directory,
      cache_directory: gallerySettings.cache_directory,
      copyright_holder: enableTextWatermark ? (gallerySettings.copyright_holder || undefined) : undefined,
      image_watermark: enableImageWatermark ? gallerySettings.image_watermark : undefined,
      enable_tile_zoom: enableTileZoom,
      grid_mode: gallerySettings.grid_mode || 'masonry',
      max_columns: gallerySettings.max_columns,
    };
    updateGalleryMutation.mutate(request);
  };

  const loadWatermarkFolder = useCallback(async () => {
    setWatermarkFolderLoading(true);
    try {
      const result = await api.ensureWatermarkFolder(name);
      setWatermarkImages(result.images);
    } catch (err) {
      console.error('Failed to load watermark folder:', err);
    } finally {
      setWatermarkFolderLoading(false);
    }
  }, [name]);

  // Load watermark images when image watermark is enabled and settings are shown
  useEffect(() => {
    if (enableImageWatermark && showWatermarkSettings && watermarkImages.length === 0 && !watermarkFolderLoading) {
      loadWatermarkFolder();
    }
  }, [enableImageWatermark, showWatermarkSettings, watermarkImages.length, watermarkFolderLoading, loadWatermarkFolder]);

  // Auto-open folder modal if initialFolderPath is provided via URL
  useEffect(() => {
    if (initialFolderPath && foldersData && !initialFolderOpened) {
      // Decode the folder path from URL
      const decodedPath = decodeURIComponent(initialFolderPath);
      // Find the folder in the list (empty string for root)
      const folder = foldersData.folders.find(
        (f) => f.path === decodedPath || (decodedPath === '_root' && f.path === '')
      );
      if (folder) {
        setEditFolder(folder);
        setInitialFolderOpened(true);
      }
    }
  }, [initialFolderPath, foldersData, initialFolderOpened]);

  // When modal closes, navigate back appropriately
  const handleFolderModalClose = () => {
    setEditFolder(null);
    if (initialFolderPath && data) {
      // User came from main site - redirect back there
      const folderPath = initialFolderPath === '_root' ? '' : decodeURIComponent(initialFolderPath);
      const mainSiteUrl = folderPath
        ? `${data.url_prefix}/${folderPath}`
        : data.url_prefix;
      window.location.href = mainSiteUrl;
    }
  };

  const permissions = localPermissions ?? sitePermissions ?? data?.permissions;

  if (isLoading) {
    return <div className="loading">Loading gallery...</div>;
  }

  if (error) {
    return <div className="error">Failed to load gallery: {String(error)}</div>;
  }

  if (!data || !permissions) {
    return <div className="error">Gallery not found</div>;
  }

  const updateLocalPermissions = (updater: (p: PermissionConfig) => PermissionConfig) => {
    const current = localPermissions ?? sitePermissions ?? data.permissions;
    setLocalPermissions(updater(current));
    setHasChanges(true);
  };

  const handleSave = () => {
    if (localPermissions) {
      updatePermissionsMutation.mutate(localPermissions);
    }
  };

  const availableRoles = getAvailableRoles(permissions.roles);

  const handleAddUserAssignment = () => {
    if (!newUserAssignment.username.trim()) return;
    updateLocalPermissions((p) => ({
      ...p,
      user_roles: [
        ...p.user_roles.filter((u) => u.username !== newUserAssignment.username),
        { username: newUserAssignment.username, roles: newUserAssignment.roles },
      ],
    }));
    setNewUserAssignment({ username: '', roles: ['viewer'] });
    setShowAddUser(false);
  };

  const handleDeleteUserAssignment = (username: string) => {
    updateLocalPermissions((p) => ({
      ...p,
      user_roles: p.user_roles.filter((u) => u.username !== username),
    }));
  };

  return (
    <div>
      <header className="page-header">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: '1rem' }}>
            <Link to="/galleries" className="btn btn-secondary">
              Back
            </Link>
            <div>
              <h1 className="page-title">{name}</h1>
              <p className="page-subtitle">Gallery permissions and settings</p>
            </div>
          </div>
          {hasChanges && (
            <button
              className="btn btn-primary"
              onClick={handleSave}
              disabled={updatePermissionsMutation.isPending}
            >
              {updatePermissionsMutation.isPending ? 'Saving...' : 'Save Changes'}
            </button>
          )}
        </div>
      </header>

      {updatePermissionsMutation.isSuccess && (
        <div className="card" style={{ background: 'var(--color-success)', color: 'white', marginBottom: '1rem' }}>
          Permissions saved successfully
        </div>
      )}

      {updatePermissionsMutation.error && (
        <div className="error" style={{ marginBottom: '1rem' }}>
          Failed to save: {String(updatePermissionsMutation.error)}
        </div>
      )}

      {/* Gallery Settings */}
      {gallerySettings && (
        <div className="card">
          <div className="card-header" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <span>Gallery Settings</span>
            {hasSettingsChanges && (
              <button
                className="btn btn-primary btn-sm"
                onClick={handleSaveSettings}
                disabled={updateGalleryMutation.isPending}
              >
                {updateGalleryMutation.isPending ? 'Saving...' : 'Save Settings'}
              </button>
            )}
          </div>

          {updateGalleryMutation.isSuccess && (
            <div style={{ background: 'var(--color-success)', color: 'white', padding: '0.5rem 1rem', marginBottom: '1rem' }}>
              Gallery settings saved successfully
            </div>
          )}

          {updateGalleryMutation.error && (
            <div className="error" style={{ marginBottom: '1rem' }}>
              Failed to save: {String(updateGalleryMutation.error)}
            </div>
          )}

          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '1rem' }}>
            <div className="form-group">
              <label className="form-label">URL Prefix</label>
              <input
                type="text"
                className="form-input"
                value={gallerySettings.url_prefix}
                onChange={(e) => updateGallerySettings({ url_prefix: e.target.value })}
                pattern="/[a-zA-Z0-9/_-]*"
                title="Must start with / (e.g., /gallery, /photos)"
              />
            </div>
            {!hostedMode && (
              <>
                <div className="form-group">
                  <label className="form-label">Source Directory</label>
                  <input
                    type="text"
                    className="form-input"
                    value={gallerySettings.source_directory}
                    onChange={(e) => updateGallerySettings({ source_directory: e.target.value })}
                  />
                </div>
                <div className="form-group">
                  <label className="form-label">Cache Directory</label>
                  <input
                    type="text"
                    className="form-input"
                    value={gallerySettings.cache_directory}
                    onChange={(e) => updateGallerySettings({ cache_directory: e.target.value })}
                  />
                </div>
              </>
            )}
          </div>
          {/* Layout Settings */}
          <div style={{ borderTop: '1px solid var(--color-border)', marginTop: '1rem', paddingTop: '1rem' }}>
            <h4 style={{ margin: '0 0 0.75rem 0' }}>Layout Settings</h4>
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '1rem' }}>
              <div className="form-group">
                <label className="form-label">Grid Mode</label>
                <select
                  className="form-input"
                  value={gallerySettings.grid_mode || 'masonry'}
                  onChange={(e) => updateGallerySettings({ grid_mode: e.target.value })}
                >
                  <option value="masonry">Masonry</option>
                  <option value="square">Square</option>
                </select>
                <small style={{ color: 'var(--color-text-muted)' }}>
                  Masonry uses variable-height images; Square uses uniform grid cells
                </small>
              </div>
              <div className="form-group">
                <label className="form-label">Max Columns</label>
                <input
                  type="number"
                  className="form-input"
                  min="1"
                  max="5"
                  value={gallerySettings.max_columns ?? ''}
                  onChange={(e) => updateGallerySettings({ max_columns: e.target.value ? Number(e.target.value) : undefined })}
                  placeholder="Default (2)"
                />
                <small style={{ color: 'var(--color-text-muted)' }}>
                  Maximum number of columns in the grid layout (default: 2)
                </small>
              </div>
            </div>
          </div>

          {/* Tile Zoom Toggle */}
          <div className="form-group" style={{ marginTop: '1rem' }}>
            <label style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', cursor: 'pointer' }}>
              <input
                type="checkbox"
                checked={enableTileZoom}
                onChange={(e) => {
                  setEnableTileZoom(e.target.checked);
                  setHasSettingsChanges(true);
                }}
              />
              <strong>Enable Tile Zoom</strong>
            </label>
            <small style={{ color: 'var(--color-text-muted)', display: 'block', marginTop: '0.25rem' }}>
              Generate tile-based deep zoom for high-resolution image viewing
            </small>
          </div>

          {/* Watermark Settings Toggle */}
          <div style={{ borderTop: '1px solid var(--color-border)', marginTop: '1rem', paddingTop: '1rem' }}>
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => setShowWatermarkSettings(!showWatermarkSettings)}
              style={{ width: '100%', textAlign: 'left', display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}
            >
              <span>Watermark Settings</span>
              <span>{showWatermarkSettings ? '▼' : '▶'}</span>
            </button>
          </div>

          {showWatermarkSettings && (
            <div style={{ marginTop: '1rem', padding: '1rem', background: 'var(--color-bg-secondary)', borderRadius: '4px' }}>
              {/* Text Watermark Toggle */}
              <div className="form-group">
                <label style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', cursor: 'pointer' }}>
                  <input
                    type="checkbox"
                    checked={enableTextWatermark}
                    onChange={(e) => {
                      setEnableTextWatermark(e.target.checked);
                      setHasSettingsChanges(true);
                    }}
                  />
                  <strong>Enable Text Watermark</strong>
                </label>
                <small style={{ color: 'var(--color-text-muted)', display: 'block', marginTop: '0.25rem' }}>
                  Adds a copyright text watermark to medium-sized images
                </small>
              </div>

              {enableTextWatermark && (
                <div style={{ marginTop: '0.5rem', paddingLeft: '1rem', borderLeft: '2px solid var(--color-border)' }}>
                  <div className="form-group">
                    <label className="form-label">Copyright Holder</label>
                    <input
                      type="text"
                      className="form-input"
                      value={gallerySettings.copyright_holder || ''}
                      onChange={(e) => updateGallerySettings({ copyright_holder: e.target.value || undefined })}
                      placeholder="e.g., John Doe Photography"
                    />
                  </div>
                </div>
              )}

              {/* Image Watermark Toggle */}
              <div className="form-group" style={{ marginTop: '1rem' }}>
                <label style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', cursor: 'pointer' }}>
                  <input
                    type="checkbox"
                    checked={enableImageWatermark}
                    onChange={(e) => {
                      setEnableImageWatermark(e.target.checked);
                      setHasSettingsChanges(true);
                      if (e.target.checked) {
                        if (!gallerySettings.image_watermark) {
                          updateWatermark({ image: '' });
                        }
                        // Auto-load watermark folder when enabling
                        loadWatermarkFolder();
                      }
                    }}
                  />
                  <strong>Enable Image Watermark</strong>
                </label>
                <small style={{ color: 'var(--color-text-muted)', display: 'block', marginTop: '0.25rem' }}>
                  Overlay a PNG image as a watermark
                </small>
              </div>

              {enableImageWatermark && (
                <div style={{ marginTop: '1rem', paddingLeft: '1rem', borderLeft: '2px solid var(--color-border)' }}>
                  <div className="form-group">
                    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '0.5rem' }}>
                      <label className="form-label" style={{ margin: 0 }}>Watermark Image</label>
                      <div style={{ display: 'flex', gap: '0.5rem' }}>
                        <button
                          type="button"
                          className="btn btn-secondary btn-sm"
                          onClick={loadWatermarkFolder}
                          disabled={watermarkFolderLoading}
                          title="Refresh list of available watermark images"
                        >
                          {watermarkFolderLoading ? 'Loading...' : '↻'}
                        </button>
                        {data && (
                          <a
                            href={`${data.url_prefix}/_watermark`}
                            target="_blank"
                            rel="noopener noreferrer"
                            className="btn btn-secondary btn-sm"
                            title="Open gallery folder to upload images"
                          >
                            Upload
                          </a>
                        )}
                      </div>
                    </div>
                    <select
                      className="form-input"
                      value={gallerySettings.image_watermark?.image || ''}
                      onChange={(e) => updateWatermark({ image: e.target.value })}
                    >
                      <option value="">{watermarkFolderLoading ? 'Loading...' : watermarkImages.length === 0 ? 'No images yet' : 'Select an image...'}</option>
                      {watermarkImages.map((img) => (
                        <option key={img.path} value={img.path}>
                          {img.filename}
                        </option>
                      ))}
                    </select>
                    <small style={{ color: 'var(--color-text-muted)' }}>
                      {watermarkImages.length > 0
                        ? 'Select a watermark image. Click "Upload" to add more.'
                        : 'Click "Upload" to add watermark images to the _watermark folder.'}
                    </small>
                  </div>

                  <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '1rem' }}>
                    <div className="form-group">
                      <label className="form-label">Position</label>
                      <select
                        className="form-input"
                        value={gallerySettings.image_watermark?.position || 'bottom_right'}
                        onChange={(e) => updateWatermark({ position: e.target.value as WatermarkPosition })}
                      >
                        {WATERMARK_POSITIONS.map((pos) => (
                          <option key={pos.value} value={pos.value}>
                            {pos.label}
                          </option>
                        ))}
                      </select>
                    </div>

                    <div className="form-group">
                      <label className="form-label">Opacity ({Math.round((gallerySettings.image_watermark?.opacity ?? 0.5) * 100)}%)</label>
                      <input
                        type="range"
                        className="form-input"
                        min="0"
                        max="1"
                        step="0.05"
                        value={gallerySettings.image_watermark?.opacity ?? 0.5}
                        onChange={(e) => updateWatermark({ opacity: parseFloat(e.target.value) })}
                        style={{ padding: 0 }}
                      />
                    </div>

                    <div className="form-group">
                      <label className="form-label">Scale ({gallerySettings.image_watermark?.scale ?? 15}%)</label>
                      <input
                        type="range"
                        className="form-input"
                        min="5"
                        max="50"
                        step="1"
                        value={gallerySettings.image_watermark?.scale ?? 15}
                        onChange={(e) => updateWatermark({ scale: parseInt(e.target.value) })}
                        style={{ padding: 0 }}
                      />
                      <small style={{ color: 'var(--color-text-muted)' }}>
                        Percentage of smaller image dimension
                      </small>
                    </div>

                    <div className="form-group">
                      <label className="form-label">Padding ({gallerySettings.image_watermark?.padding ?? 10}px)</label>
                      <input
                        type="range"
                        className="form-input"
                        min="0"
                        max="50"
                        step="5"
                        value={gallerySettings.image_watermark?.padding ?? 10}
                        onChange={(e) => updateWatermark({ padding: parseInt(e.target.value) })}
                        style={{ padding: 0 }}
                      />
                      <small style={{ color: 'var(--color-text-muted)' }}>
                        Distance from image edge
                      </small>
                    </div>
                  </div>

                  <div className="form-group" style={{ marginTop: '0.5rem' }}>
                    <label style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', cursor: 'pointer' }}>
                      <input
                        type="checkbox"
                        checked={gallerySettings.image_watermark?.adaptive ?? true}
                        onChange={(e) => updateWatermark({ adaptive: e.target.checked })}
                      />
                      <span>Adaptive (auto-invert on light backgrounds)</span>
                    </label>
                    <small style={{ color: 'var(--color-text-muted)', display: 'block', marginTop: '0.25rem' }}>
                      Automatically inverts grayscale watermarks when placed on light areas
                    </small>
                  </div>

                  <div className="form-group" style={{ marginTop: '1rem' }}>
                    <label className="form-label">Apply Watermark To</label>
                    <div style={{ display: 'flex', flexDirection: 'column', gap: '0.5rem', marginTop: '0.5rem' }}>
                      <label style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', cursor: 'pointer' }}>
                        <input
                          type="checkbox"
                          checked={gallerySettings.image_watermark?.apply_to_gallery ?? false}
                          onChange={(e) => updateWatermark({ apply_to_gallery: e.target.checked })}
                        />
                        <span>Gallery size (grid view)</span>
                      </label>
                      <label style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', cursor: 'pointer' }}>
                        <input
                          type="checkbox"
                          checked={gallerySettings.image_watermark?.apply_to_medium ?? true}
                          onChange={(e) => updateWatermark({ apply_to_medium: e.target.checked })}
                        />
                        <span>Medium size (detail view)</span>
                      </label>
                      <label style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', cursor: 'pointer' }}>
                        <input
                          type="checkbox"
                          checked={gallerySettings.image_watermark?.apply_to_large ?? false}
                          onChange={(e) => updateWatermark({ apply_to_large: e.target.checked })}
                        />
                        <span>Large size (download)</span>
                      </label>
                    </div>
                    <small style={{ color: 'var(--color-text-muted)', display: 'block', marginTop: '0.25rem' }}>
                      Select which image sizes should have watermarks applied
                    </small>
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
      )}

      <div className="card">
        <div className="card-header">Access Settings</div>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '1rem' }}>
          <div className="form-group">
            <label className="form-label">Public Role</label>
            <select
              className="form-input"
              value={permissions.public_role || 'none'}
              onChange={(e) =>
                updateLocalPermissions((p) => ({
                  ...p,
                  public_role: e.target.value === 'none' ? null : e.target.value,
                }))
              }
            >
              <option value="none">None (private gallery)</option>
              {availableRoles.map((role) => (
                <option key={role} value={role}>
                  {role}
                </option>
              ))}
            </select>
            <small style={{ color: 'var(--color-text-muted)' }}>
              Role assigned to unauthenticated visitors
            </small>
          </div>
          <div className="form-group">
            <label className="form-label">Default Authenticated Role</label>
            <select
              className="form-input"
              value={permissions.default_authenticated_role || 'viewer'}
              onChange={(e) =>
                updateLocalPermissions((p) => ({
                  ...p,
                  default_authenticated_role: e.target.value,
                }))
              }
            >
              {availableRoles.map((role) => (
                <option key={role} value={role}>
                  {role}
                </option>
              ))}
            </select>
            <small style={{ color: 'var(--color-text-muted)' }}>
              Role assigned to logged-in users without specific assignment
            </small>
          </div>
        </div>
      </div>

      <div className="card">
        <div className="card-header" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <span>User Role Assignments</span>
          <button className="btn btn-primary btn-sm" onClick={() => setShowAddUser(true)}>
            + Add Assignment
          </button>
        </div>
        {permissions.user_roles.length > 0 ? (
          <table className="table">
            <thead>
              <tr>
                <th>Username</th>
                <th>Role(s)</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {permissions.user_roles.map((assignment) => (
                <tr key={assignment.username}>
                  <td>{assignment.username}</td>
                  <td>
                    {assignment.roles.map((role) => (
                      <span key={role} className="badge badge-success" style={{ marginRight: '0.25rem' }}>
                        {role}
                      </span>
                    ))}
                  </td>
                  <td>
                    <button
                      className="btn btn-danger btn-sm"
                      onClick={() => handleDeleteUserAssignment(assignment.username)}
                    >
                      Remove
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <p style={{ color: 'var(--color-text-muted)', padding: '1rem' }}>
            No user-specific role assignments. All users will receive the default authenticated role.
          </p>
        )}
      </div>

      <div className="card">
        <div className="card-header" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <span>Folder Permissions</span>
          <button
            className="btn btn-primary btn-sm"
            onClick={() => {
              setCreateFolderParent('');
              setShowCreateFolder(true);
            }}
          >
            + Create Folder
          </button>
        </div>
        <p style={{ color: 'var(--color-text-muted)', padding: '0 1rem', marginBottom: '1rem' }}>
          Override permissions for specific folders within this gallery.
        </p>
        {foldersData && foldersData.folders.length > 0 ? (
          <table className="table">
            <thead>
              <tr>
                <th>Folder</th>
                <th>Images</th>
                <th>Size</th>
                <th>Custom Permissions</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {foldersData.folders.map((folder) => (
                <tr key={folder.path}>
                  <td>
                    <code>{folder.name}</code>
                    {folder.path && (
                      <small style={{ display: 'block', color: 'var(--color-text-muted)' }}>
                        {folder.path}
                      </small>
                    )}
                  </td>
                  <td>{folder.image_count.toLocaleString()}</td>
                  <td>{folder.size_formatted}</td>
                  <td>
                    {folder.has_custom_permissions ? (
                      <span className="badge badge-warning">Custom</span>
                    ) : (
                      <span className="badge badge-secondary">Inherited</span>
                    )}
                  </td>
                  <td>
                    <button
                      className="btn btn-secondary btn-sm"
                      onClick={() => setEditFolder(folder)}
                    >
                      Edit
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <p style={{ color: 'var(--color-text-muted)', padding: '1rem' }}>
            No folders found in this gallery.
          </p>
        )}
      </div>

      {/* Add User Assignment Modal */}
      {showAddUser && (
        <div className="modal-overlay" onClick={() => setShowAddUser(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">Add User Assignment</div>
            <div className="form-group">
              <label className="form-label">Username</label>
              <select
                className="form-input"
                value={newUserAssignment.username}
                onChange={(e) => setNewUserAssignment({ ...newUserAssignment, username: e.target.value })}
              >
                <option value="">Select user...</option>
                {usersData?.users.map((user) => (
                  <option key={user.username} value={user.username}>
                    {user.username} ({user.email})
                  </option>
                ))}
              </select>
            </div>
            <div className="form-group">
              <label className="form-label">Role</label>
              <select
                className="form-input"
                value={newUserAssignment.roles[0]}
                onChange={(e) => setNewUserAssignment({ ...newUserAssignment, roles: [e.target.value] })}
              >
                {availableRoles.map((role) => (
                  <option key={role} value={role}>
                    {role}
                  </option>
                ))}
              </select>
            </div>
            <div className="modal-actions">
              <button className="btn btn-secondary" onClick={() => setShowAddUser(false)}>
                Cancel
              </button>
              <button
                className="btn btn-primary"
                onClick={handleAddUserAssignment}
                disabled={!newUserAssignment.username}
              >
                Add Assignment
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Edit Folder Permissions Modal */}
      {editFolder && (
        <FolderPermissionsModal
          galleryName={name}
          folder={editFolder}
          availableRoles={availableRoles}
          fullScreen={!!initialFolderPath}
          onClose={handleFolderModalClose}
          onSaved={() => {
            queryClient.invalidateQueries({ queryKey: ['galleryFolders'] });
            handleFolderModalClose();
          }}
        />
      )}

      {/* Create Folder Modal */}
      {showCreateFolder && (
        <CreateFolderModal
          galleryName={name}
          parentFolder={createFolderParent}
          onClose={() => setShowCreateFolder(false)}
          onCreated={() => {
            queryClient.invalidateQueries({ queryKey: ['galleryFolders'] });
            setShowCreateFolder(false);
          }}
        />
      )}
    </div>
  );
}

function CreateFolderModal({
  galleryName,
  parentFolder,
  onClose,
  onCreated,
}: {
  galleryName: string;
  parentFolder: string;
  onClose: () => void;
  onCreated: () => void;
}) {
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;

    setCreating(true);
    setError(null);
    try {
      await api.createFolder(galleryName, parentFolder, {
        name: name.trim(),
        description: description.trim() || undefined,
      });
      onCreated();
    } catch (err) {
      setError(String(err));
      setCreating(false);
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">Create Folder</div>
        <form onSubmit={handleCreate}>
          <div className="form-group">
            <label className="form-label">Parent Folder</label>
            <input
              type="text"
              className="form-input"
              value={parentFolder || '(root)'}
              disabled
              style={{ background: 'var(--color-bg-secondary)' }}
            />
          </div>
          <div className="form-group">
            <label className="form-label">Folder Name</label>
            <input
              type="text"
              className="form-input"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="new-folder"
              pattern="[a-zA-Z0-9_-]+"
              title="Only letters, numbers, hyphens, and underscores allowed"
              required
              autoFocus
            />
            <small style={{ color: 'var(--color-text-muted)' }}>
              Only letters, numbers, hyphens, and underscores
            </small>
          </div>
          <div className="form-group">
            <label className="form-label">Description (optional)</label>
            <textarea
              className="form-input"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={2}
              placeholder="Optional description for this folder..."
            />
          </div>
          {error && (
            <div className="error" style={{ marginBottom: '1rem' }}>
              {error}
            </div>
          )}
          <div className="modal-actions">
            <button type="button" className="btn btn-secondary" onClick={onClose}>
              Cancel
            </button>
            <button type="submit" className="btn btn-primary" disabled={creating || !name.trim()}>
              {creating ? 'Creating...' : 'Create Folder'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

function FolderPermissionsModal({
  galleryName,
  folder,
  availableRoles,
  fullScreen = false,
  onClose,
  onSaved,
}: {
  galleryName: string;
  folder: FolderInfo;
  availableRoles: string[];
  fullScreen?: boolean;
  onClose: () => void;
  onSaved: () => void;
}) {
  const queryClient = useQueryClient();
  const [loading, setLoading] = useState(true);
  const [hidden, setHidden] = useState(false);
  const [permissions, setPermissions] = useState<PermissionConfig>({
    site_admins: [],
    public_role: null,
    default_authenticated_role: null,
    roles: {},
    user_roles: [],
  });
  const [description, setDescription] = useState('');
  const [gridMode, setGridMode] = useState<string | undefined>(undefined);
  const [maxColumns, setMaxColumns] = useState<number | undefined>(undefined);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Share form state
  const [shareEmail, setShareEmail] = useState('');
  const [shareRole, setShareRole] = useState('viewer');
  const [sharing, setSharing] = useState(false);
  const [shareMessage, setShareMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

  // Add user assignment state
  const [showAddUser, setShowAddUser] = useState(false);
  const [newUsername, setNewUsername] = useState('');
  const [newUserRole, setNewUserRole] = useState('viewer');

  // Create subfolder state
  const [showCreateSubfolder, setShowCreateSubfolder] = useState(false);

  // Delete folder state
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const [deleting, setDeleting] = useState(false);

  // Fetch users for dropdown
  const { data: usersData } = useQuery({
    queryKey: ['users'],
    queryFn: api.listUsers,
  });

  // Load current folder permissions
  useState(() => {
    api
      .getFolderPermissions(DEFAULT_SITE, galleryName, folder.path)
      .then((data) => {
        setHidden(data.hidden);
        setPermissions(data.permissions);
        setDescription(data.description);
        setGridMode(data.grid_mode);
        setMaxColumns(data.max_columns);
        setLoading(false);
      })
      .catch((err) => {
        setError(String(err));
        setLoading(false);
      });
  });

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    try {
      await api.updateFolderPermissions(DEFAULT_SITE, galleryName, folder.path, {
        hidden,
        permissions,
        description,
        grid_mode: gridMode,
        max_columns: maxColumns,
      });
      onSaved();
    } catch (err) {
      setError(String(err));
      setSaving(false);
    }
  };

  const handleDeleteFolder = async () => {
    setDeleting(true);
    setError(null);
    try {
      await api.deleteFolder(galleryName, folder.path);
      queryClient.invalidateQueries({ queryKey: ['galleryFolders'] });
      onClose();
    } catch (err) {
      setError(String(err));
      setDeleting(false);
      setShowDeleteConfirm(false);
    }
  };

  const canDeleteFolder = folder.image_count === 0 && folder.path !== '';

  const handleShare = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!shareEmail.trim()) return;

    setSharing(true);
    setShareMessage(null);
    try {
      const result = await api.shareFolder(DEFAULT_SITE, galleryName, folder.path, {
        email: shareEmail,
        role: shareRole,
      });
      setShareMessage({ type: 'success', text: result.message });
      setShareEmail('');
      // Reload permissions to show the new user assignment
      const data = await api.getFolderPermissions(DEFAULT_SITE, galleryName, folder.path);
      setPermissions(data.permissions);
    } catch (err) {
      setShareMessage({ type: 'error', text: String(err) });
    } finally {
      setSharing(false);
    }
  };

  const modalStyle = fullScreen
    ? { width: '100vw', height: '100vh', maxWidth: '100vw', maxHeight: '100vh', margin: 0, borderRadius: 0, overflow: 'auto' }
    : { maxWidth: '800px', width: '90vw', maxHeight: '90vh', overflow: 'auto' };

  return (
    <div className="modal-overlay" onClick={onClose} style={fullScreen ? { padding: 0 } : undefined}>
      <div className="modal" style={modalStyle} onClick={(e) => e.stopPropagation()}>
        <div className="modal-header" style={fullScreen ? { position: 'sticky', top: 0, background: 'var(--color-bg-primary)', zIndex: 10, borderBottom: '1px solid var(--color-border)' } : undefined}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
            <div>
              Folder: {folder.name}
              <small style={{ display: 'block', color: 'var(--color-text-muted)', fontWeight: 'normal' }}>
                {folder.path || '(root)'}
              </small>
            </div>
            <button
              className="btn btn-secondary btn-sm"
              onClick={() => setShowCreateSubfolder(true)}
              style={{ marginLeft: '1rem' }}
            >
              + Create Subfolder
            </button>
          </div>
        </div>

        {loading ? (
          <div className="loading">Loading folder permissions...</div>
        ) : (
          <>
            {error && (
              <div className="error" style={{ marginBottom: '1rem' }}>
                {error}
              </div>
            )}

            {/* Share Section */}
            <div style={{ borderBottom: '1px solid var(--color-border)', paddingBottom: '1rem', marginBottom: '1rem' }}>
              <h4 style={{ margin: '0 0 0.5rem 0', color: 'var(--color-text-muted)' }}>Share This Folder</h4>
              <form onSubmit={handleShare}>
                <div style={{ display: 'grid', gridTemplateColumns: '1fr auto auto', gap: '0.5rem', alignItems: 'end' }}>
                  <div className="form-group" style={{ margin: 0 }}>
                    <label className="form-label" style={{ fontSize: '0.85rem' }}>Email</label>
                    <input
                      type="email"
                      className="form-input"
                      value={shareEmail}
                      onChange={(e) => setShareEmail(e.target.value)}
                      placeholder="collaborator@example.com"
                      required
                    />
                  </div>
                  <div className="form-group" style={{ margin: 0 }}>
                    <label className="form-label" style={{ fontSize: '0.85rem' }}>Role</label>
                    <select
                      className="form-input"
                      value={shareRole}
                      onChange={(e) => setShareRole(e.target.value)}
                    >
                      {availableRoles.map((role) => (
                        <option key={role} value={role}>
                          {role}
                        </option>
                      ))}
                    </select>
                  </div>
                  <button type="submit" className="btn btn-primary" disabled={sharing || !shareEmail.trim()}>
                    {sharing ? 'Sending...' : 'Share'}
                  </button>
                </div>
              </form>
              {shareMessage && (
                <div
                  style={{
                    marginTop: '0.5rem',
                    padding: '0.5rem',
                    borderRadius: '4px',
                    background: shareMessage.type === 'success' ? 'var(--color-success-bg)' : 'var(--color-error-bg)',
                    color: shareMessage.type === 'success' ? 'var(--color-success)' : 'var(--color-error)',
                    fontSize: '0.9rem',
                  }}
                >
                  {shareMessage.text}
                </div>
              )}
              <small style={{ color: 'var(--color-text-muted)', display: 'block', marginTop: '0.5rem' }}>
                Sends an invitation email with a link to access this folder.
                Creates an account if the email is not registered.
              </small>
            </div>

            {/* Permissions Section */}
            <h4 style={{ margin: '0 0 0.5rem 0', color: 'var(--color-text-muted)' }}>Permissions</h4>

            <div className="form-group">
              <label style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
                <input
                  type="checkbox"
                  checked={hidden}
                  onChange={(e) => setHidden(e.target.checked)}
                />
                <strong>Hidden</strong>
              </label>
              <small style={{ color: 'var(--color-text-muted)' }}>
                Hidden folders are not shown in gallery listings
              </small>
            </div>

            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '1rem' }}>
              <div className="form-group">
                <label className="form-label">Grid Mode Override</label>
                <select
                  className="form-input"
                  value={gridMode || ''}
                  onChange={(e) => setGridMode(e.target.value || undefined)}
                >
                  <option value="">Inherit from gallery</option>
                  <option value="masonry">Masonry</option>
                  <option value="square">Square</option>
                </select>
              </div>
              <div className="form-group">
                <label className="form-label">Max Columns Override</label>
                <input
                  type="number"
                  className="form-input"
                  min={1}
                  max={5}
                  placeholder="Inherit from gallery"
                  value={maxColumns ?? ''}
                  onChange={(e) => setMaxColumns(e.target.value ? Number(e.target.value) : undefined)}
                />
              </div>
            </div>

            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '1rem' }}>
              <div className="form-group">
                <label className="form-label">Public Role Override</label>
                <select
                  className="form-input"
                  value={permissions.public_role || ''}
                  onChange={(e) =>
                    setPermissions({
                      ...permissions,
                      public_role: e.target.value || null,
                    })
                  }
                >
                  <option value="">Inherit from gallery</option>
                  <option value="none">None (private)</option>
                  {availableRoles.map((role) => (
                    <option key={role} value={role}>
                      {role}
                    </option>
                  ))}
                </select>
              </div>
              <div className="form-group">
                <label className="form-label">Default Auth Role Override</label>
                <select
                  className="form-input"
                  value={permissions.default_authenticated_role || ''}
                  onChange={(e) =>
                    setPermissions({
                      ...permissions,
                      default_authenticated_role: e.target.value || null,
                    })
                  }
                >
                  <option value="">Inherit from gallery</option>
                  {availableRoles.map((role) => (
                    <option key={role} value={role}>
                      {role}
                    </option>
                  ))}
                </select>
              </div>
            </div>

            {/* User Assignments Section */}
            <div style={{ borderTop: '1px solid var(--color-border)', paddingTop: '1rem', marginTop: '1rem' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '0.5rem' }}>
                <label className="form-label" style={{ margin: 0 }}>User Assignments (folder-specific)</label>
                <button
                  className="btn btn-secondary btn-sm"
                  onClick={() => setShowAddUser(true)}
                >
                  + Add User
                </button>
              </div>
              <small style={{ color: 'var(--color-text-muted)', display: 'block', marginBottom: '0.5rem' }}>
                Assign site-level roles to specific users for this folder.
              </small>

              {showAddUser && (
                <div style={{ display: 'flex', gap: '0.5rem', marginBottom: '0.5rem', padding: '0.5rem', background: 'var(--color-bg-secondary)', borderRadius: '4px' }}>
                  <select
                    className="form-input"
                    style={{ flex: 1 }}
                    value={newUsername}
                    onChange={(e) => setNewUsername(e.target.value)}
                  >
                    <option value="">Select user...</option>
                    {usersData?.users
                      .filter((u) => !permissions.user_roles.some((ur) => ur.username === u.username))
                      .map((user) => (
                        <option key={user.username} value={user.username}>
                          {user.username} ({user.email})
                        </option>
                      ))}
                  </select>
                  <select
                    className="form-input"
                    style={{ width: 'auto' }}
                    value={newUserRole}
                    onChange={(e) => setNewUserRole(e.target.value)}
                  >
                    {availableRoles.map((role) => (
                      <option key={role} value={role}>
                        {role}
                      </option>
                    ))}
                  </select>
                  <button
                    className="btn btn-primary btn-sm"
                    disabled={!newUsername}
                    onClick={() => {
                      if (newUsername) {
                        setPermissions({
                          ...permissions,
                          user_roles: [
                            ...permissions.user_roles,
                            { username: newUsername, roles: [newUserRole] },
                          ],
                        });
                        setNewUsername('');
                        setNewUserRole('viewer');
                        setShowAddUser(false);
                      }
                    }}
                  >
                    Add
                  </button>
                  <button
                    className="btn btn-secondary btn-sm"
                    onClick={() => {
                      setShowAddUser(false);
                      setNewUsername('');
                    }}
                  >
                    Cancel
                  </button>
                </div>
              )}

              {permissions.user_roles.length > 0 ? (
                <div style={{ marginBottom: '0.5rem' }}>
                  {permissions.user_roles.map((ur, i) => (
                    <div
                      key={ur.username}
                      style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', marginBottom: '0.25rem' }}
                    >
                      <span style={{ minWidth: '120px' }}>{ur.username}</span>
                      <select
                        className="form-input"
                        style={{ width: 'auto', flex: 1 }}
                        value={ur.roles[0]}
                        onChange={(e) => {
                          const newUserRoles = [...permissions.user_roles];
                          newUserRoles[i] = { ...ur, roles: [e.target.value] };
                          setPermissions({ ...permissions, user_roles: newUserRoles });
                        }}
                      >
                        {availableRoles.map((role) => (
                          <option key={role} value={role}>
                            {role}
                          </option>
                        ))}
                      </select>
                      <button
                        className="btn btn-danger btn-sm"
                        onClick={() => {
                          setPermissions({
                            ...permissions,
                            user_roles: permissions.user_roles.filter((_, j) => j !== i),
                          });
                        }}
                      >
                        Remove
                      </button>
                    </div>
                  ))}
                </div>
              ) : (
                <p style={{ color: 'var(--color-text-muted)', fontSize: '0.9rem', margin: '0.5rem 0' }}>
                  No folder-specific user assignments.
                </p>
              )}
            </div>

            <div className="form-group" style={{ marginTop: '1rem' }}>
              <label className="form-label">Description (Markdown)</label>
              <textarea
                className="form-input"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                rows={3}
                placeholder="Optional description shown in the gallery..."
              />
            </div>

            {/* Create Subfolder Modal */}
            {showCreateSubfolder && (
              <CreateFolderModal
                galleryName={galleryName}
                parentFolder={folder.path}
                onClose={() => setShowCreateSubfolder(false)}
                onCreated={() => {
                  queryClient.invalidateQueries({ queryKey: ['galleryFolders'] });
                  setShowCreateSubfolder(false);
                }}
              />
            )}
          </>
        )}

        <div className="modal-actions" style={fullScreen ? { position: 'sticky', bottom: 0, background: 'var(--color-bg-primary)', zIndex: 10, borderTop: '1px solid var(--color-border)', padding: '1rem', margin: 0 } : undefined}>
          {canDeleteFolder && (
            <button
              className="btn btn-danger"
              onClick={() => setShowDeleteConfirm(true)}
              disabled={loading || saving || deleting}
              style={{ marginRight: 'auto' }}
            >
              Delete Folder
            </button>
          )}
          <button className="btn btn-secondary" onClick={onClose}>
            Cancel
          </button>
          <button className="btn btn-primary" onClick={handleSave} disabled={loading || saving}>
            {saving ? 'Saving...' : 'Save'}
          </button>
        </div>

        {/* Delete Folder Confirmation Modal */}
        {showDeleteConfirm && (
          <div className="modal-overlay" onClick={() => setShowDeleteConfirm(false)} style={{ zIndex: 1001 }}>
            <div className="modal" onClick={(e) => e.stopPropagation()} style={{ maxWidth: '400px' }}>
              <div className="modal-header">Delete Folder</div>
              <p style={{ margin: '1rem 0' }}>
                Are you sure you want to delete the folder <strong>{folder.name}</strong>?
              </p>
              <p style={{ color: 'var(--color-error)', fontSize: '0.9rem' }}>
                This action cannot be undone.
              </p>
              <div className="modal-actions">
                <button className="btn btn-secondary" onClick={() => setShowDeleteConfirm(false)}>
                  Cancel
                </button>
                <button
                  className="btn btn-danger"
                  onClick={handleDeleteFolder}
                  disabled={deleting}
                >
                  {deleting ? 'Deleting...' : 'Delete'}
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

