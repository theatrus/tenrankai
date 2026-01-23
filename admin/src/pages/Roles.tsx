import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { api, RoleInfo, RolePermissions, PermissionConfig } from '@api/client';

const DEFAULT_SITE = 'default';

const PERMISSION_CATEGORIES = [
  {
    name: 'Viewing',
    permissions: [
      { key: 'can_view', label: 'View gallery' },
      { key: 'can_see_technical_details', label: 'Technical details' },
      { key: 'can_see_exact_dates', label: 'Exact dates' },
      { key: 'can_see_location', label: 'Location info' },
    ],
  },
  {
    name: 'Downloads',
    permissions: [
      { key: 'can_download_medium', label: 'Medium' },
      { key: 'can_download_large', label: 'Large' },
      { key: 'can_download_original', label: 'Original' },
      { key: 'can_download_gallery', label: 'Gallery ZIP' },
      { key: 'can_download_raw', label: 'RAW files' },
    ],
  },
  {
    name: 'Versions',
    permissions: [{ key: 'can_see_versions', label: 'See versions' }],
  },
  {
    name: 'Metadata',
    permissions: [
      { key: 'can_read_metadata', label: 'Read metadata' },
      { key: 'can_edit_content', label: 'Edit content' },
    ],
  },
  {
    name: 'Comments',
    permissions: [
      { key: 'can_add_comments', label: 'Add' },
      { key: 'can_edit_own_comments', label: 'Edit own' },
      { key: 'can_delete_own_comments', label: 'Delete own' },
      { key: 'can_edit_any_comments', label: 'Edit any' },
      { key: 'can_delete_any_comments', label: 'Delete any' },
    ],
  },
  {
    name: 'Organization',
    permissions: [
      { key: 'can_set_picks', label: 'Set picks' },
      { key: 'can_add_tags', label: 'Add tags' },
      { key: 'can_manage_images', label: 'Manage images' },
    ],
  },
  {
    name: 'Zoom',
    permissions: [
      { key: 'can_use_zoom', label: 'Use zoom' },
      { key: 'can_use_tile_zoom', label: 'Tile zoom' },
    ],
  },
  {
    name: 'AI Features',
    permissions: [
      { key: 'can_analyze_images', label: 'Analyze images' },
      { key: 'can_see_ai_analysis', label: 'See AI analysis' },
      { key: 'can_see_ai_alt_text', label: 'See AI alt text' },
    ],
  },
  {
    name: 'Admin',
    permissions: [{ key: 'owner_access', label: 'Owner access' }],
  },
];

const BUILTIN_ROLES = ['viewer', 'contributor', 'admin'];

function emptyPermissions(): RolePermissions {
  return {
    can_view: false,
    can_see_technical_details: false,
    can_see_exact_dates: false,
    can_see_location: false,
    can_download_medium: false,
    can_download_large: false,
    can_download_original: false,
    can_download_gallery: false,
    can_download_raw: false,
    can_see_versions: false,
    can_read_metadata: false,
    can_edit_content: false,
    can_add_comments: false,
    can_edit_own_comments: false,
    can_delete_own_comments: false,
    can_edit_any_comments: false,
    can_delete_any_comments: false,
    can_manage_images: false,
    can_set_picks: false,
    can_add_tags: false,
    can_use_zoom: false,
    can_use_tile_zoom: false,
    can_analyze_images: false,
    can_see_ai_analysis: false,
    can_see_ai_alt_text: false,
    owner_access: false,
  };
}

export function Roles() {
  const queryClient = useQueryClient();
  const [editRole, setEditRole] = useState<{ name: string; role: RoleInfo } | null>(null);
  const [showAddRole, setShowAddRole] = useState(false);
  const [newRoleName, setNewRoleName] = useState('');

  // Fetch built-in roles
  const { data: builtinRolesData, isLoading: builtinLoading } = useQuery({
    queryKey: ['roles'],
    queryFn: api.listRoles,
  });

  // Fetch site permissions (contains custom roles)
  const { data: sitePermissions, isLoading: siteLoading, error } = useQuery({
    queryKey: ['sitePermissions', DEFAULT_SITE],
    queryFn: () => api.getSitePermissions(DEFAULT_SITE),
  });

  const updatePermissionsMutation = useMutation({
    mutationFn: (permissions: PermissionConfig) => api.updateSitePermissions(DEFAULT_SITE, permissions),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['sitePermissions'] });
    },
  });

  if (error) {
    return <div className="error">Failed to load roles: {String(error)}</div>;
  }

  const isLoading = builtinLoading || siteLoading;

  // Combine built-in roles with custom roles from site permissions
  const builtinRoles = builtinRolesData?.roles || [];
  const customRoles = sitePermissions?.roles || {};

  const allRoleNames = new Set([
    ...BUILTIN_ROLES,
    ...Object.keys(customRoles),
  ]);

  const handleAddRole = () => {
    if (!newRoleName.trim() || !sitePermissions) return;
    const newRole: RoleInfo = {
      name: newRoleName,
      permissions: emptyPermissions(),
      inherits: 'viewer',
      is_builtin: false,
    };
    updatePermissionsMutation.mutate({
      ...sitePermissions,
      roles: {
        ...sitePermissions.roles,
        [newRoleName]: newRole,
      },
    });
    setNewRoleName('');
    setShowAddRole(false);
  };

  const handleDeleteRole = (roleName: string) => {
    if (!sitePermissions) return;
    if (!confirm(`Delete role "${roleName}"? Users assigned to this role will lose these permissions.`)) return;
    const { [roleName]: _, ...rest } = sitePermissions.roles;
    updatePermissionsMutation.mutate({
      ...sitePermissions,
      roles: rest,
    });
  };

  const handleSaveRole = (roleName: string, role: RoleInfo) => {
    if (!sitePermissions) return;
    updatePermissionsMutation.mutate({
      ...sitePermissions,
      roles: {
        ...sitePermissions.roles,
        [roleName]: role,
      },
    });
    setEditRole(null);
  };

  return (
    <div>
      <header className="page-header">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <div>
            <h1 className="page-title">Roles</h1>
            <p className="page-subtitle">Manage permission roles for all galleries</p>
          </div>
          <button className="btn btn-primary" onClick={() => setShowAddRole(true)}>
            + Add Custom Role
          </button>
        </div>
      </header>

      {updatePermissionsMutation.isSuccess && (
        <div className="card" style={{ background: 'var(--color-success)', color: 'white', marginBottom: '1rem' }}>
          Roles saved successfully
        </div>
      )}

      {updatePermissionsMutation.error && (
        <div className="error" style={{ marginBottom: '1rem' }}>
          Failed to save: {String(updatePermissionsMutation.error)}
        </div>
      )}

      {isLoading ? (
        <div className="loading">Loading roles...</div>
      ) : (
        <div style={{ display: 'grid', gap: '1rem' }}>
          {/* Built-in Roles */}
          <h3 style={{ margin: '0.5rem 0', color: 'var(--color-text-muted)' }}>Built-in Roles</h3>
          {builtinRoles.map((role: RoleInfo) => (
            <RoleCard
              key={role.name}
              role={role}
              onEdit={() => setEditRole({ name: role.name, role })}
            />
          ))}

          {/* Custom Roles */}
          <h3 style={{ margin: '1rem 0 0.5rem 0', color: 'var(--color-text-muted)' }}>Custom Roles</h3>
          {Object.entries(customRoles).filter(([name]) => !BUILTIN_ROLES.includes(name)).length > 0 ? (
            Object.entries(customRoles)
              .filter(([name]) => !BUILTIN_ROLES.includes(name))
              .map(([name, role]) => (
                <RoleCard
                  key={name}
                  role={{ ...role, name }}
                  onEdit={() => setEditRole({ name, role: { ...role, name } })}
                  onDelete={() => handleDeleteRole(name)}
                />
              ))
          ) : (
            <div className="card" style={{ color: 'var(--color-text-muted)', textAlign: 'center', padding: '2rem' }}>
              No custom roles defined. Click "Add Custom Role" to create one.
            </div>
          )}
        </div>
      )}

      {/* Add Role Modal */}
      {showAddRole && (
        <div className="modal-overlay" onClick={() => setShowAddRole(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">Add Custom Role</div>
            <div className="form-group">
              <label className="form-label">Role Name</label>
              <input
                type="text"
                className="form-input"
                value={newRoleName}
                onChange={(e) => setNewRoleName(e.target.value)}
                placeholder="e.g., client, editor, premium"
                pattern="[a-zA-Z0-9_-]+"
              />
              <small style={{ color: 'var(--color-text-muted)' }}>
                Lowercase letters, numbers, underscores, and hyphens only
              </small>
            </div>
            <div className="modal-actions">
              <button className="btn btn-secondary" onClick={() => setShowAddRole(false)}>
                Cancel
              </button>
              <button
                className="btn btn-primary"
                onClick={handleAddRole}
                disabled={!newRoleName.trim() || updatePermissionsMutation.isPending}
              >
                {updatePermissionsMutation.isPending ? 'Creating...' : 'Add Role'}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Edit Role Modal */}
      {editRole && (
        <RoleEditorModal
          roleName={editRole.name}
          role={editRole.role}
          availableRoles={Array.from(allRoleNames)}
          onSave={(role) => handleSaveRole(editRole.name, role)}
          onClose={() => setEditRole(null)}
        />
      )}
    </div>
  );
}

function RoleCard({
  role,
  onEdit,
  onDelete,
}: {
  role: RoleInfo;
  onEdit: () => void;
  onDelete?: () => void;
}) {
  const enabledPermissions = Object.entries(role.permissions).filter(([, v]) => v === true);

  return (
    <div className="card">
      <div className="card-header" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
          <span style={{ fontWeight: 600 }}>{role.name}</span>
          {role.is_builtin && (
            <span className="badge badge-secondary" style={{ fontSize: '0.625rem' }}>
              Built-in
            </span>
          )}
          {role.inherits && (
            <span style={{ color: 'var(--color-text-muted)', fontSize: '0.85rem' }}>
              (inherits: {role.inherits})
            </span>
          )}
        </div>
        <div className="actions">
          <button className="btn btn-secondary btn-sm" onClick={onEdit}>
            {role.is_builtin ? 'View' : 'Edit'}
          </button>
          {onDelete && !role.is_builtin && (
            <button className="btn btn-danger btn-sm" onClick={onDelete}>
              Delete
            </button>
          )}
        </div>
      </div>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: '0.5rem', padding: '0.5rem 0' }}>
        {enabledPermissions.length > 0 ? (
          enabledPermissions.map(([key]) => (
            <span key={key} className="badge badge-success">
              {formatPermissionName(key)}
            </span>
          ))
        ) : (
          <span style={{ color: 'var(--color-text-muted)', fontStyle: 'italic' }}>
            No permissions enabled
          </span>
        )}
      </div>
    </div>
  );
}

function RoleEditorModal({
  roleName,
  role,
  availableRoles,
  onSave,
  onClose,
}: {
  roleName: string;
  role: RoleInfo;
  availableRoles: string[];
  onSave: (role: RoleInfo) => void;
  onClose: () => void;
}) {
  const [editedRole, setEditedRole] = useState<RoleInfo>({ ...role });

  const togglePermission = (key: string) => {
    setEditedRole({
      ...editedRole,
      permissions: {
        ...editedRole.permissions,
        [key]: !editedRole.permissions[key as keyof RolePermissions],
      },
    });
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" style={{ maxWidth: '600px' }} onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          {role.is_builtin ? 'View' : 'Edit'} Role: {roleName}
          {role.is_builtin && <span className="badge badge-secondary" style={{ marginLeft: '0.5rem' }}>Built-in</span>}
        </div>

        <div className="form-group">
          <label className="form-label">Inherits From</label>
          <select
            className="form-input"
            value={editedRole.inherits || ''}
            onChange={(e) => setEditedRole({ ...editedRole, inherits: e.target.value || null })}
            disabled={role.is_builtin}
          >
            <option value="">None</option>
            {availableRoles.filter((r) => r !== roleName).map((r) => (
              <option key={r} value={r}>
                {r}
              </option>
            ))}
          </select>
          <small style={{ color: 'var(--color-text-muted)' }}>
            Inherited permissions are automatically included
          </small>
        </div>

        <div style={{ maxHeight: '400px', overflowY: 'auto' }}>
          {PERMISSION_CATEGORIES.map((category) => (
            <div key={category.name} style={{ marginBottom: '1rem' }}>
              <div style={{ fontWeight: 'bold', marginBottom: '0.5rem', color: 'var(--color-text-muted)' }}>
                {category.name}
              </div>
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: '0.5rem' }}>
                {category.permissions.map((perm) => (
                  <label
                    key={perm.key}
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '0.25rem',
                      padding: '0.25rem 0.5rem',
                      background: editedRole.permissions[perm.key as keyof RolePermissions]
                        ? 'var(--color-success-bg)'
                        : 'var(--color-bg-secondary)',
                      borderRadius: '4px',
                      cursor: role.is_builtin ? 'not-allowed' : 'pointer',
                      opacity: role.is_builtin ? 0.7 : 1,
                    }}
                  >
                    <input
                      type="checkbox"
                      checked={editedRole.permissions[perm.key as keyof RolePermissions]}
                      onChange={() => togglePermission(perm.key)}
                      disabled={role.is_builtin}
                    />
                    {perm.label}
                  </label>
                ))}
              </div>
            </div>
          ))}
        </div>

        <div className="modal-actions">
          <button className="btn btn-secondary" onClick={onClose}>
            {role.is_builtin ? 'Close' : 'Cancel'}
          </button>
          {!role.is_builtin && (
            <button className="btn btn-primary" onClick={() => onSave(editedRole)}>
              Save Role
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function formatPermissionName(permission: string): string {
  return permission
    .replace(/^can_/, '')
    .replace(/_/g, ' ')
    .replace(/\b\w/g, (c) => c.toUpperCase());
}
