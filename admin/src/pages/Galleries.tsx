import { useQuery } from '@tanstack/react-query';
import { Link, useParams } from 'react-router-dom';
import { api, GalleryInfo } from '@api/client';

export function Galleries() {
  const { name } = useParams<{ name: string }>();

  const { data, isLoading, error } = useQuery({
    queryKey: ['galleries'],
    queryFn: api.listGalleries,
  });

  if (error) {
    return <div className="error">Failed to load galleries: {String(error)}</div>;
  }

  if (name) {
    return <GalleryDetail name={name} />;
  }

  return (
    <div>
      <header className="page-header">
        <h1 className="page-title">Galleries</h1>
        <p className="page-subtitle">Manage gallery settings and permissions</p>
      </header>

      <div className="card">
        {isLoading ? (
          <div className="loading">Loading galleries...</div>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Name</th>
                <th>URL Prefix</th>
                <th>Public Role</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {data?.galleries.map((gallery: GalleryInfo) => (
                <tr key={gallery.name}>
                  <td>{gallery.name}</td>
                  <td>
                    <code>{gallery.url_prefix}</code>
                  </td>
                  <td>
                    <span className={`badge ${gallery.permissions.public_role ? 'badge-success' : 'badge-warning'}`}>
                      {gallery.permissions.public_role || 'none (private)'}
                    </span>
                  </td>
                  <td>
                    <Link to={`/galleries/${gallery.name}`} className="btn btn-secondary">
                      Manage
                    </Link>
                  </td>
                </tr>
              ))}
              {data?.galleries.length === 0 && (
                <tr>
                  <td colSpan={4} style={{ textAlign: 'center', color: 'var(--color-text-muted)' }}>
                    No galleries configured
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}

function GalleryDetail({ name }: { name: string }) {
  const { data, isLoading, error } = useQuery({
    queryKey: ['gallery', name],
    queryFn: () => api.getGallery(name),
  });

  if (isLoading) {
    return <div className="loading">Loading gallery...</div>;
  }

  if (error) {
    return <div className="error">Failed to load gallery: {String(error)}</div>;
  }

  if (!data) {
    return <div className="error">Gallery not found</div>;
  }

  return (
    <div>
      <header className="page-header">
        <div style={{ display: 'flex', alignItems: 'center', gap: '1rem' }}>
          <Link to="/galleries" className="btn btn-secondary">
            Back
          </Link>
          <div>
            <h1 className="page-title">{name}</h1>
            <p className="page-subtitle">Gallery permissions and settings</p>
          </div>
        </div>
      </header>

      <div className="card">
        <div className="card-header">Permissions</div>
        <div className="form-group">
          <label className="form-label">Public Role</label>
          <p style={{ color: 'var(--color-text-muted)' }}>
            {data.permissions.public_role || 'None (private gallery)'}
          </p>
        </div>
        <div className="form-group">
          <label className="form-label">Default Authenticated Role</label>
          <p style={{ color: 'var(--color-text-muted)' }}>
            {data.permissions.default_authenticated_role || 'viewer'}
          </p>
        </div>
      </div>

      <div className="card">
        <div className="card-header">Defined Roles</div>
        {Object.keys(data.permissions.roles).length > 0 ? (
          <table className="table">
            <thead>
              <tr>
                <th>Role Name</th>
                <th>Inherits</th>
                <th>Permissions</th>
              </tr>
            </thead>
            <tbody>
              {Object.entries(data.permissions.roles).map(([roleName, role]) => (
                <tr key={roleName}>
                  <td>{roleName}</td>
                  <td>{role.inherits || '-'}</td>
                  <td>
                    <span className="badge badge-success">
                      {Object.entries(role.permissions).filter(([, v]) => v === true).length} enabled
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <p style={{ color: 'var(--color-text-muted)' }}>No custom roles defined</p>
        )}
      </div>

      <div className="card">
        <div className="card-header">User Role Assignments</div>
        {data.permissions.user_roles.length > 0 ? (
          <table className="table">
            <thead>
              <tr>
                <th>Username</th>
                <th>Role(s)</th>
              </tr>
            </thead>
            <tbody>
              {data.permissions.user_roles.map((assignment) => (
                <tr key={assignment.username}>
                  <td>{assignment.username}</td>
                  <td>
                    {assignment.roles.map((role) => (
                      <span key={role} className="badge badge-success" style={{ marginRight: '0.25rem' }}>
                        {role}
                      </span>
                    ))}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <p style={{ color: 'var(--color-text-muted)' }}>No user-specific role assignments</p>
        )}
      </div>
    </div>
  );
}
