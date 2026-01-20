import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { api, UserInfo, CreateUserRequest } from '@api/client';

export function Users() {
  const queryClient = useQueryClient();
  const [showModal, setShowModal] = useState(false);
  const [newUser, setNewUser] = useState<CreateUserRequest>({
    username: '',
    email: '',
    send_invite: true,
  });

  const { data, isLoading, error } = useQuery({
    queryKey: ['users'],
    queryFn: api.listUsers,
  });

  const createMutation = useMutation({
    mutationFn: api.createUser,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['users'] });
      setShowModal(false);
      setNewUser({ username: '', email: '', send_invite: true });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: api.deleteUser,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['users'] });
    },
  });

  const [inviteSuccess, setInviteSuccess] = useState<string | null>(null);

  const inviteMutation = useMutation({
    mutationFn: api.sendInvite,
    onSuccess: (_, username) => {
      setInviteSuccess(username);
      setTimeout(() => setInviteSuccess(null), 3000);
    },
  });

  const handleCreate = (e: React.FormEvent) => {
    e.preventDefault();
    createMutation.mutate(newUser);
  };

  const handleDelete = (username: string) => {
    if (confirm(`Are you sure you want to delete user "${username}"?`)) {
      deleteMutation.mutate(username);
    }
  };

  const handleInvite = (username: string) => {
    inviteMutation.mutate(username);
  };

  if (error) {
    return <div className="error">Failed to load users: {String(error)}</div>;
  }

  return (
    <div>
      <header className="page-header">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <div>
            <h1 className="page-title">Users</h1>
            <p className="page-subtitle">Manage user accounts</p>
          </div>
          <button className="btn btn-primary" onClick={() => setShowModal(true)}>
            Add User
          </button>
        </div>
      </header>

      {inviteSuccess && (
        <div className="card" style={{ background: 'var(--color-success)', color: 'white', marginBottom: '1rem' }}>
          Invite sent to {inviteSuccess}
        </div>
      )}

      <div className="card">
        {isLoading ? (
          <div className="loading">Loading users...</div>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Username</th>
                <th>Email</th>
                <th>Passkeys</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {data?.users.map((user: UserInfo) => (
                <tr key={user.username}>
                  <td>{user.username}</td>
                  <td>{user.email}</td>
                  <td>
                    <span className={`badge ${user.passkey_count > 0 ? 'badge-success' : 'badge-warning'}`}>
                      {user.passkey_count} passkey{user.passkey_count !== 1 ? 's' : ''}
                    </span>
                  </td>
                  <td>
                    <div className="actions">
                      <button
                        className="btn btn-secondary"
                        onClick={() => handleInvite(user.username)}
                        disabled={inviteMutation.isPending}
                      >
                        Send Invite
                      </button>
                      <button
                        className="btn btn-danger"
                        onClick={() => handleDelete(user.username)}
                        disabled={deleteMutation.isPending}
                      >
                        Delete
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
              {data?.users.length === 0 && (
                <tr>
                  <td colSpan={4} style={{ textAlign: 'center', color: 'var(--color-text-muted)' }}>
                    No users found
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        )}
      </div>

      {showModal && (
        <div className="modal-overlay" onClick={() => setShowModal(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">Add User</div>
            <form onSubmit={handleCreate}>
              <div className="form-group">
                <label className="form-label">Username</label>
                <input
                  type="text"
                  className="form-input"
                  value={newUser.username}
                  onChange={(e) => setNewUser({ ...newUser, username: e.target.value })}
                  required
                  pattern="[a-zA-Z0-9_]{3,32}"
                  title="3-32 alphanumeric characters or underscores"
                />
              </div>
              <div className="form-group">
                <label className="form-label">Email</label>
                <input
                  type="email"
                  className="form-input"
                  value={newUser.email}
                  onChange={(e) => setNewUser({ ...newUser, email: e.target.value })}
                  required
                />
              </div>
              <div className="form-group">
                <label style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
                  <input
                    type="checkbox"
                    checked={newUser.send_invite}
                    onChange={(e) => setNewUser({ ...newUser, send_invite: e.target.checked })}
                  />
                  Send invite email
                </label>
              </div>
              {createMutation.error && (
                <div className="error" style={{ marginBottom: '1rem' }}>
                  {String(createMutation.error)}
                </div>
              )}
              <div className="modal-actions">
                <button type="button" className="btn btn-secondary" onClick={() => setShowModal(false)}>
                  Cancel
                </button>
                <button type="submit" className="btn btn-primary" disabled={createMutation.isPending}>
                  {createMutation.isPending ? 'Creating...' : 'Create User'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}
