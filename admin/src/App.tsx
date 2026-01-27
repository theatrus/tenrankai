import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { AdminLayout } from '@components/layout/AdminLayout';
import { Dashboard } from '@pages/Dashboard';
import { Users } from '@pages/Users';
import { Galleries } from '@pages/Galleries';
import { Roles } from '@pages/Roles';
import { Theme } from '@pages/Theme';

export function App() {
  return (
    <BrowserRouter basename="/_admin">
      <Routes>
        <Route path="/" element={<AdminLayout />}>
          <Route index element={<Dashboard />} />
          <Route path="users" element={<Users />} />
          <Route path="galleries" element={<Galleries />} />
          <Route path="galleries/:name" element={<Galleries />} />
          <Route path="galleries/:name/folders/*" element={<Galleries />} />
          <Route path="roles" element={<Roles />} />
          <Route path="theme" element={<Theme />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}
