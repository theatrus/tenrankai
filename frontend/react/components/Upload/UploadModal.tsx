import React, { useEffect, useMemo, useRef, useState } from 'react';
import Uppy from '@uppy/core';
import Dashboard from '@uppy/dashboard';
import Tus from '@uppy/tus';
import '@uppy/core/css/style.min.css';
import '@uppy/dashboard/css/style.min.css';

interface UploadModalProps {
  galleryName: string;
  folderPath: string;
  onClose: () => void;
  onSuccess: () => void;
}

const ALLOWED_FILE_TYPES = [
  // Images
  '.jpg', '.jpeg', '.png', '.webp', '.avif', '.heic', '.heif', '.gif',
  // RAW formats
  '.raw', '.cr2', '.cr3', '.nef', '.arw', '.dng', '.orf', '.rw2', '.raf', '.pef',
  // Sidecar files
  '.md', '.xmp',
];

// Primary image extensions that should upload first
const PRIMARY_EXTENSIONS = new Set([
  'jpg', 'jpeg', 'png', 'webp', 'avif', 'heic', 'heif', 'gif',
]);

// Sidecar/RAW extensions that require a primary image
const SIDECAR_EXTENSIONS = new Set([
  'raw', 'cr2', 'cr3', 'nef', 'arw', 'dng', 'orf', 'rw2', 'raf', 'pef',
  'md', 'xmp',
]);

// Get file extension in lowercase
function getExtension(filename: string): string {
  const ext = filename.split('.').pop()?.toLowerCase();
  return ext || '';
}

// Get base name without extension
function getBaseName(filename: string): string {
  const lastDot = filename.lastIndexOf('.');
  return lastDot > 0 ? filename.substring(0, lastDot) : filename;
}

// Check if file is a primary image
function isPrimaryImage(filename: string): boolean {
  return PRIMARY_EXTENSIONS.has(getExtension(filename));
}

// Check if file is a sidecar that needs a primary
function isSidecarFile(filename: string): boolean {
  return SIDECAR_EXTENSIONS.has(getExtension(filename));
}

const MAX_FILE_SIZE = 500 * 1024 * 1024; // 500MB

export const UploadModal: React.FC<UploadModalProps> = ({
  galleryName,
  folderPath,
  onClose,
  onSuccess,
}) => {
  const dashboardRef = useRef<HTMLDivElement>(null);
  const [error, setError] = useState<string | null>(null);
  const [uploadComplete, setUploadComplete] = useState(false);
  const [successCount, setSuccessCount] = useState(0);

  // Track files with missing primaries for warning display
  const [sidecarWarnings, setSidecarWarnings] = useState<string[]>([]);

  const uppy = useMemo(() => {
    const instance = new Uppy({
      id: 'gallery-uploader',
      restrictions: {
        maxFileSize: MAX_FILE_SIZE,
        allowedFileTypes: ALLOWED_FILE_TYPES,
      },
      autoProceed: false,
    });

    instance.use(Tus, {
      endpoint: `/_upload/${galleryName}`,
      chunkSize: 5 * 1024 * 1024, // 5MB chunks
      retryDelays: [0, 1000, 3000, 5000],
      limit: 1, // Upload one file at a time to ensure primaries complete before sidecars
      headers: {},
      onBeforeRequest: (req, file) => {
        // Build metadata string with filename and folder path
        const parts: string[] = [];
        if (file.name) {
          parts.push(`filename ${btoa(file.name)}`);
        }
        if (folderPath) {
          parts.push(`folderPath ${btoa(folderPath)}`);
        }
        if (parts.length > 0) {
          req.setHeader('Upload-Metadata', parts.join(','));
        }
      },
    });

    return instance;
  }, [galleryName, folderPath]);

  useEffect(() => {
    if (dashboardRef.current) {
      uppy.use(Dashboard, {
        inline: true,
        target: dashboardRef.current,
        height: 350,
        width: '100%',
        proudlyDisplayPoweredByUppy: false,
        hideProgressAfterFinish: false,
        note: `Supported: Images (JPEG, PNG, HEIC, AVIF), RAW files, Markdown. Max ${MAX_FILE_SIZE / 1024 / 1024}MB per file.`,
      });
    }

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const handleComplete = (result: any) => {
      const successful = result.successful?.length || 0;
      const failed = result.failed?.length || 0;

      setUploadComplete(true);
      setSuccessCount(successful);

      if (failed > 0) {
        // Show error details from failed uploads
        const errorMessages = result.failed
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          ?.map((file: any) => {
            const errorMsg = file.error || file.response?.body || 'Unknown error';
            return `${file.name}: ${errorMsg}`;
          })
          .join('\n');
        setError(`${failed} file(s) failed to upload:\n${errorMessages}`);
      } else if (successful > 0) {
        // All uploads succeeded - don't auto-refresh, let user click Done
        setError(null);
      }
    };

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const handleUploadError = (file: any, error: any, response: any) => {
      console.error('Upload error:', { file: file?.name, error, response });
      const errorMsg = response?.body || error?.message || 'Upload failed';
      setError(`Upload failed for ${file?.name || 'file'}: ${errorMsg}`);
    };

    // Validate sidecars have matching primaries when files are added
    // Remove any sidecar/RAW files that don't have a matching primary image
    const handleFilesAdded = () => {
      const files = uppy.getFiles();
      const rejected: string[] = [];

      // Get all base names that have primaries
      const primaryBaseNames = new Set(
        files
          .filter(f => isPrimaryImage(f.name))
          .map(f => getBaseName(f.name).toLowerCase())
      );

      // Remove sidecars without matching primaries
      for (const file of files) {
        if (isSidecarFile(file.name)) {
          const baseName = getBaseName(file.name).toLowerCase();
          if (!primaryBaseNames.has(baseName)) {
            rejected.push(file.name);
            uppy.removeFile(file.id);
          }
        }
      }

      setSidecarWarnings(rejected);
    };

    uppy.on('file-added', handleFilesAdded);
    uppy.on('file-removed', handleFilesAdded);
    uppy.on('complete', handleComplete);
    uppy.on('upload-error', handleUploadError);

    return () => {
      uppy.off('file-added', handleFilesAdded);
      uppy.off('file-removed', handleFilesAdded);
      uppy.off('complete', handleComplete);
      uppy.off('upload-error', handleUploadError);
      uppy.destroy();
    };
  }, [uppy]);

  const handleDone = () => {
    if (uploadComplete && successCount > 0) {
      onSuccess();
    } else {
      onClose();
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div
        className="modal-content upload-modal"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: '700px', width: '90%', position: 'relative' }}
      >
        <div className="modal-header" style={{ marginBottom: '1rem' }}>
          <h3 style={{ margin: 0 }}>
            Upload Images
            {folderPath && (
              <span style={{ fontWeight: 'normal', fontSize: '0.9em', opacity: 0.7 }}>
                {' '}to /{folderPath}
              </span>
            )}
          </h3>
          <button
            className="modal-close"
            onClick={onClose}
            style={{
              position: 'absolute',
              top: '1rem',
              right: '1rem',
              background: 'none',
              border: 'none',
              fontSize: '1.5rem',
              cursor: 'pointer',
              padding: '0.5rem',
            }}
            aria-label="Close"
          >
            &times;
          </button>
        </div>
        <div ref={dashboardRef} />
        {sidecarWarnings.length > 0 && (
          <div
            style={{
              marginTop: '1rem',
              padding: '12px',
              backgroundColor: 'var(--message-warning-bg)',
              border: '1px solid var(--message-warning-border)',
              borderRadius: '4px',
              color: 'var(--message-warning-color)',
              fontSize: '0.9em',
            }}
          >
            <strong>Removed:</strong> The following sidecar/RAW files were removed because they have no matching primary image:
            <ul style={{ margin: '0.5rem 0 0 1rem', padding: 0 }}>
              {sidecarWarnings.map(name => (
                <li key={name}>{name}</li>
              ))}
            </ul>
            <div style={{ marginTop: '0.5rem', fontSize: '0.85em' }}>
              Add a matching image file (same name, different extension) to upload these files.
            </div>
          </div>
        )}
        {error && (
          <div
            style={{
              marginTop: '1rem',
              padding: '12px',
              backgroundColor: 'var(--message-error-bg)',
              border: '1px solid var(--message-error-border)',
              borderRadius: '4px',
              color: 'var(--message-error-color)',
              fontSize: '0.9em',
              whiteSpace: 'pre-wrap',
            }}
          >
            {error}
          </div>
        )}
        {uploadComplete && successCount > 0 && !error && (
          <div
            style={{
              marginTop: '1rem',
              padding: '12px',
              backgroundColor: 'var(--message-success-bg)',
              border: '1px solid var(--message-success-border)',
              borderRadius: '4px',
              color: 'var(--message-success-color)',
              fontSize: '0.9em',
            }}
          >
            Successfully uploaded {successCount} file{successCount > 1 ? 's' : ''}. Click Done to refresh the gallery.
          </div>
        )}
        <div className="modal-actions" style={{ marginTop: '1rem', textAlign: 'right' }}>
          <button className="btn btn-secondary" onClick={handleDone}>
            {uploadComplete && successCount > 0 ? 'Done' : 'Close'}
          </button>
        </div>
      </div>
    </div>
  );
};
