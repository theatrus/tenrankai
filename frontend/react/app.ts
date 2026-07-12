// Main entry point that imports all components
// Each component self-initializes when the DOM is ready

// Import theme toggle (runs on all pages)
import './theme-toggle.ts';
import './components/user-menu.ts';

// Import page-specific components
// These check for their mount points and only initialize if found
import './pages/gallery.tsx';
import './pages/image-detail.tsx';
import './pages/gallery-preview-template.ts';
import './pages/login.ts';
import './pages/login-success.ts';
import './pages/passkey-enrollment.ts';
import './pages/passkeys.ts';
import './pages/gallery-image-hover.tsx';
import './pages/post-detail.tsx';
import './pages/posts-index.tsx';
import './pages/posts-preview.tsx';
import './pages/profile.ts';

// Export a global object for any runtime configuration if needed
(window as any).Tenrankai = {
  version: '1.0.0',
  initialized: true
};
