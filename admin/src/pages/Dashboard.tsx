import { useQuery } from '@tanstack/react-query';
import { api } from '@api/client';
import { useHostedMode } from '@hooks/useHostedMode';

export function Dashboard() {
  const hostedMode = useHostedMode();

  const { data: users, isLoading: usersLoading } = useQuery({
    queryKey: ['users'],
    queryFn: api.listUsers,
  });

  const { data: galleries, isLoading: galleriesLoading } = useQuery({
    queryKey: ['galleries'],
    queryFn: api.listGalleries,
  });

  return (
    <div>
      <header className="page-header">
        <h1 className="page-title">Dashboard</h1>
        <p className="page-subtitle">{hostedMode ? 'Site administration overview' : 'Server administration overview'}</p>
      </header>

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: '1rem' }}>
        <div className="card">
          <div className="card-header">Users</div>
          {usersLoading ? (
            <div className="loading">Loading...</div>
          ) : (
            <div style={{ fontSize: '2rem', fontWeight: '700' }}>
              {users?.users.length ?? 0}
            </div>
          )}
        </div>

        <div className="card">
          <div className="card-header">Galleries</div>
          {galleriesLoading ? (
            <div className="loading">Loading...</div>
          ) : (
            <div style={{ fontSize: '2rem', fontWeight: '700' }}>
              {galleries?.galleries.length ?? 0}
            </div>
          )}
        </div>

        <div className="card">
          <div className="card-header">Status</div>
          <div style={{ fontSize: '1rem' }}>
            <span className="badge badge-success">Online</span>
          </div>
        </div>
      </div>
    </div>
  );
}
