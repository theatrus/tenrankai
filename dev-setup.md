# Development Workflow Guide

## React + Vite Development Setup

This project uses a hybrid architecture with Rust SSR and React progressive enhancement.

### Development Servers

You need to run two servers during development:

#### 1. Rust Server (Backend + SSR)
```bash
# Terminal 1: Start Rust server
cargo run -- --host 0.0.0.0 --port 3000

# With debug logging
RUST_LOG=debug cargo run -- --host 0.0.0.0 --port 3000
```

#### 2. Vite Dev Server (React HMR)
```bash
# Terminal 2: Start Vite dev server
npm run dev
# Runs on http://localhost:5173 with proxy to Rust server
```

### Development URLs

- **Main Development URL**: http://localhost:5173
  - All requests proxy to Rust server
  - React bundles served with HMR
  - Source maps and debugging enabled

- **Direct Rust Server**: http://localhost:3000
  - Direct access to server-rendered pages
  - No React HMR (for testing SSR)

### Build Commands

#### Development
```bash
# Skip frontend build (fastest)
cargo build

# Force frontend build in debug mode
TENRANKAI_BUILD_FRONTEND=1 cargo build

# Skip frontend build entirely
TENRANKAI_SKIP_FRONTEND=1 cargo build
```

#### Production
```bash
# Automatic frontend build with optimizations
cargo build --release

# Manual frontend build
npm run build:prod
```

### Testing React Components

```bash
# Build React components manually
npm run build

# Preview built components
npm run preview

# Development with HMR
npm run dev
```

### File Watching

The Rust build system automatically detects changes in:
- `frontend/react/**` (Vite frontend)
- `package.json`, `tsconfig.json`, `vite.config.js`

### Troubleshooting

#### React Bundle Not Loading
1. Check if Vite dev server is running on port 5173
2. Verify proxy settings in vite.config.js
3. Check browser console for loading errors

#### HMR Not Working
1. Ensure both servers are running
2. Check that you're accessing via http://localhost:5173
3. Verify frontend code is in frontend/react/

#### Build Errors
```bash
# Clean and rebuild
rm -rf static/dist node_modules
npm install
TENRANKAI_BUILD_FRONTEND=1 cargo build
```

### Architecture Notes

- **Server-Side Rendering**: Liquid templates render initial HTML
- **Progressive Enhancement**: React mounts on specific DOM elements
- **Data Flow**: Server data passed via `data-*` attributes
- **Styling**: Mix of static CSS and React component styles
- **API**: React components call existing Rust API endpoints

### Next Steps

1. Test image detail page enhancement
2. Add React components to gallery page
3. Implement shared component library
4. Add React-based filtering and search
