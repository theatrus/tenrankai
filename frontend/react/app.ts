// Main entry point that imports all components
// Each component self-initializes when the DOM is ready

// Import theme toggle (runs on all pages)
import './theme-toggle.ts';

// Import page-specific components
// These check for their mount points and only initialize if found
import './pages/gallery.tsx';
import './pages/image-detail.tsx';

// Export a global object for any runtime configuration if needed
(window as any).Tenrankai = {
  version: '1.0.0',
  initialized: true
};