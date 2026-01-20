import { useQuery } from '@tanstack/react-query';
import { api, RoleInfo } from '@api/client';

const PERMISSION_GROUPS = {
  Viewing: ['can_view', 'can_see_technical_details', 'can_see_exact_dates', 'can_see_location'],
  Downloads: [
    'can_download_medium',
    'can_download_large',
    'can_download_original',
    'can_download_gallery',
    'can_download_raw',
  ],
  Versions: ['can_see_versions'],
  Metadata: ['can_read_metadata', 'can_edit_content'],
  Comments: [
    'can_add_comments',
    'can_edit_own_comments',
    'can_delete_own_comments',
    'can_edit_any_comments',
    'can_delete_any_comments',
  ],
  Features: ['can_set_picks', 'can_add_tags', 'can_use_zoom', 'can_use_tile_zoom'],
  AI: ['can_analyze_images', 'can_see_ai_analysis', 'can_see_ai_alt_text'],
  Admin: ['owner_access'],
};

export function Roles() {
  const { data, isLoading, error } = useQuery({
    queryKey: ['roles'],
    queryFn: api.listRoles,
  });

  if (error) {
    return <div className="error">Failed to load roles: {String(error)}</div>;
  }

  return (
    <div>
      <header className="page-header">
        <h1 className="page-title">Roles</h1>
        <p className="page-subtitle">View available permission roles</p>
      </header>

      {isLoading ? (
        <div className="loading">Loading roles...</div>
      ) : (
        <div style={{ display: 'grid', gap: '1rem' }}>
          {data?.roles.map((role: RoleInfo) => (
            <div key={role.name} className="card">
              <div className="card-header" style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
                {role.name}
                {role.is_builtin && (
                  <span className="badge badge-warning" style={{ fontSize: '0.625rem' }}>
                    Built-in
                  </span>
                )}
              </div>
              {role.inherits && (
                <p style={{ marginBottom: '1rem', color: 'var(--color-text-muted)' }}>
                  Inherits from: <strong>{role.inherits}</strong>
                </p>
              )}
              <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(200px, 1fr))', gap: '1rem' }}>
                {Object.entries(PERMISSION_GROUPS).map(([group, permissions]) => {
                  const enabledCount = permissions.filter(
                    (p) => role.permissions[p as keyof typeof role.permissions]
                  ).length;
                  if (enabledCount === 0) return null;
                  return (
                    <div key={group}>
                      <div style={{ fontWeight: 600, marginBottom: '0.5rem' }}>{group}</div>
                      <div style={{ display: 'flex', flexWrap: 'wrap', gap: '0.25rem' }}>
                        {permissions
                          .filter((p) => role.permissions[p as keyof typeof role.permissions])
                          .map((p) => (
                            <span key={p} className="badge badge-success">
                              {formatPermissionName(p)}
                            </span>
                          ))}
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function formatPermissionName(permission: string): string {
  return permission
    .replace(/^can_/, '')
    .replace(/_/g, ' ')
    .replace(/\b\w/g, (c) => c.toUpperCase());
}
