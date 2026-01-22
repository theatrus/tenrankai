import { useState, useEffect } from 'react';
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
} from '@api/client';

const DEFAULT_SITE = 'default';

export function Galleries() {
  const { name, '*': folderPath } = useParams<{ name: string; '*': string }>();
  const queryClient = useQueryClient();
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [editGallery, setEditGallery] = useState<SiteGalleryInfo | null>(null);
  const [newGallery, setNewGallery] = useState<CreateGalleryRequest>({
    name: '',
    url_prefix: '/gallery',
    source_directory: '',
    cache_directory: '',
    images_per_page: 50,
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
        images_per_page: 50,
      });
    },
  });

  const updateMutation = useMutation({
    mutationFn: (data: CreateGalleryRequest) => api.updateGallery(DEFAULT_SITE, data.name, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['siteGalleries'] });
      queryClient.invalidateQueries({ queryKey: ['galleries'] });
      setEditGallery(null);
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

  const handleUpdate = (e: React.FormEvent) => {
    e.preventDefault();
    if (editGallery) {
      updateMutation.mutate({
        name: editGallery.name,
        url_prefix: editGallery.url_prefix,
        source_directory: editGallery.source_directory,
        cache_directory: editGallery.cache_directory,
        images_per_page: editGallery.images_per_page,
      });
    }
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
    return <GalleryDetail name={name} initialFolderPath={folderPath} />;
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
                {hasConfigStorage && <th>Source Directory</th>}
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
                    {hasConfigStorage && (
                      <td>
                        <code>{siteGallery?.source_directory || '-'}</code>
                      </td>
                    )}
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
                          <>
                            <button
                              className="btn btn-secondary"
                              onClick={() => setEditGallery(siteGallery)}
                            >
                              Edit
                            </button>
                            <button
                              className="btn btn-danger"
                              onClick={() => handleDelete(gallery.name)}
                              disabled={deleteMutation.isPending}
                            >
                              Delete
                            </button>
                          </>
                        )}
                      </div>
                    </td>
                  </tr>
                );
              })}
              {runtimeData?.galleries.length === 0 && (
                <tr>
                  <td colSpan={hasConfigStorage ? 5 : 4} style={{ textAlign: 'center', color: 'var(--color-text-muted)' }}>
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
              <div className="form-group">
                <label className="form-label">Images Per Page</label>
                <input
                  type="number"
                  className="form-input"
                  value={newGallery.images_per_page}
                  onChange={(e) => setNewGallery({ ...newGallery, images_per_page: parseInt(e.target.value) || 50 })}
                  min={1}
                  max={500}
                />
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

      {/* Edit Gallery Modal */}
      {editGallery && (
        <div className="modal-overlay" onClick={() => setEditGallery(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">Edit Gallery: {editGallery.name}</div>
            <form onSubmit={handleUpdate}>
              <div className="form-group">
                <label className="form-label">Name</label>
                <input
                  type="text"
                  className="form-input"
                  value={editGallery.name}
                  disabled
                  style={{ background: 'var(--color-bg-secondary)' }}
                />
                <small style={{ color: 'var(--color-text-muted)' }}>
                  Gallery name cannot be changed
                </small>
              </div>
              <div className="form-group">
                <label className="form-label">URL Prefix</label>
                <input
                  type="text"
                  className="form-input"
                  value={editGallery.url_prefix}
                  onChange={(e) => setEditGallery({ ...editGallery, url_prefix: e.target.value })}
                  required
                  pattern="/[a-zA-Z0-9/_-]*"
                  title="Must start with / (e.g., /gallery, /photos)"
                />
              </div>
              <div className="form-group">
                <label className="form-label">Source Directory</label>
                <input
                  type="text"
                  className="form-input"
                  value={editGallery.source_directory}
                  onChange={(e) => setEditGallery({ ...editGallery, source_directory: e.target.value })}
                  required
                />
              </div>
              <div className="form-group">
                <label className="form-label">Cache Directory</label>
                <input
                  type="text"
                  className="form-input"
                  value={editGallery.cache_directory}
                  onChange={(e) => setEditGallery({ ...editGallery, cache_directory: e.target.value })}
                  required
                />
              </div>
              <div className="form-group">
                <label className="form-label">Images Per Page</label>
                <input
                  type="number"
                  className="form-input"
                  value={editGallery.images_per_page}
                  onChange={(e) => setEditGallery({ ...editGallery, images_per_page: parseInt(e.target.value) || 50 })}
                  min={1}
                  max={500}
                />
              </div>
              {updateMutation.error && (
                <div className="error" style={{ marginBottom: '1rem' }}>
                  {String(updateMutation.error)}
                </div>
              )}
              <div className="modal-actions">
                <button type="button" className="btn btn-secondary" onClick={() => setEditGallery(null)}>
                  Cancel
                </button>
                <button type="submit" className="btn btn-primary" disabled={updateMutation.isPending}>
                  {updateMutation.isPending ? 'Saving...' : 'Save Changes'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}

const BUILTIN_ROLES = ['viewer', 'contributor', 'admin'];

function getAvailableRoles(customRoles: Record<string, RoleInfo>): string[] {
  const roleSet = new Set(BUILTIN_ROLES);
  Object.keys(customRoles).forEach((r) => roleSet.add(r));
  return Array.from(roleSet);
}

function GalleryDetail({ name, initialFolderPath }: { name: string; initialFolderPath?: string }) {
  const queryClient = useQueryClient();
  const [showAddUser, setShowAddUser] = useState(false);
  const [newUserAssignment, setNewUserAssignment] = useState({ username: '', roles: ['viewer'] });
  const [localPermissions, setLocalPermissions] = useState<PermissionConfig | null>(null);
  const [hasChanges, setHasChanges] = useState(false);
  const [editFolder, setEditFolder] = useState<FolderInfo | null>(null);
  const [initialFolderOpened, setInitialFolderOpened] = useState(false);

  const { data, isLoading, error } = useQuery({
    queryKey: ['gallery', name],
    queryFn: () => api.getGallery(name),
  });

  const { data: sitePermissions } = useQuery({
    queryKey: ['sitePermissions', DEFAULT_SITE],
    queryFn: () => api.getSitePermissions(DEFAULT_SITE),
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
        <div className="card-header">Folder Permissions</div>
        <p style={{ color: 'var(--color-text-muted)', padding: '0 1rem', marginBottom: '1rem' }}>
          Override permissions for specific folders within this gallery.
        </p>
        {foldersData && foldersData.folders.length > 0 ? (
          <table className="table">
            <thead>
              <tr>
                <th>Folder</th>
                <th>Images</th>
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
                  <td>{folder.image_count}</td>
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
          onClose={handleFolderModalClose}
          onSaved={() => {
            queryClient.invalidateQueries({ queryKey: ['galleryFolders'] });
            handleFolderModalClose();
          }}
        />
      )}
    </div>
  );
}

function FolderPermissionsModal({
  galleryName,
  folder,
  availableRoles,
  onClose,
  onSaved,
}: {
  galleryName: string;
  folder: FolderInfo;
  availableRoles: string[];
  onClose: () => void;
  onSaved: () => void;
}) {
  const [loading, setLoading] = useState(true);
  const [hidden, setHidden] = useState(false);
  const [permissions, setPermissions] = useState<PermissionConfig>({
    public_role: null,
    default_authenticated_role: null,
    roles: {},
    user_roles: [],
  });
  const [description, setDescription] = useState('');
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
      });
      onSaved();
    } catch (err) {
      setError(String(err));
      setSaving(false);
    }
  };

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

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" style={{ maxWidth: '800px', width: '90vw', maxHeight: '90vh', overflow: 'auto' }} onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          Folder: {folder.name}
          <small style={{ display: 'block', color: 'var(--color-text-muted)', fontWeight: 'normal' }}>
            {folder.path || '(root)'}
          </small>
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
          </>
        )}

        <div className="modal-actions">
          <button className="btn btn-secondary" onClick={onClose}>
            Cancel
          </button>
          <button className="btn btn-primary" onClick={handleSave} disabled={loading || saving}>
            {saving ? 'Saving...' : 'Save'}
          </button>
        </div>
      </div>
    </div>
  );
}

