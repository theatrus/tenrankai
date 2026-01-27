import { Outlet, NavLink } from 'react-router-dom';

export function AdminLayout() {
  return (
    <div className="admin-layout">
      <aside className="sidebar">
        <div className="sidebar-header">Tenrankai Admin</div>
        <nav>
          <ul className="sidebar-nav">
            <li>
              <NavLink to="/" end>
                Dashboard
              </NavLink>
            </li>
            <li>
              <NavLink to="/users">Users</NavLink>
            </li>
            <li>
              <NavLink to="/galleries">Galleries</NavLink>
            </li>
            <li>
              <NavLink to="/roles">Roles</NavLink>
            </li>
            <li>
              <NavLink to="/theme">Theme</NavLink>
            </li>
          </ul>
        </nav>
        <div style={{ marginTop: 'auto', paddingTop: '1rem' }}>
          <a
            href="/"
            style={{
              display: 'block',
              padding: '0.75rem 1rem',
              color: 'var(--color-text-muted)',
              textDecoration: 'none',
              fontSize: '0.875rem',
            }}
          >
            Back to Site
          </a>
        </div>
      </aside>
      <main className="main-content">
        <Outlet />
      </main>
    </div>
  );
}
