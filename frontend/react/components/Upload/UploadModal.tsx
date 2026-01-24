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
  // Documents
  '.md', '.markdown',
];

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

    uppy.on('complete', handleComplete);
    uppy.on('upload-error', handleUploadError);

    return () => {
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
        {error && (
          <div
            style={{
              marginTop: '1rem',
              padding: '12px',
              backgroundColor: '#fee2e2',
              border: '1px solid #fecaca',
              borderRadius: '4px',
              color: '#dc2626',
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
              backgroundColor: '#dcfce7',
              border: '1px solid #bbf7d0',
              borderRadius: '4px',
              color: '#16a34a',
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
