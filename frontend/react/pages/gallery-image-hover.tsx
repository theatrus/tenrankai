import React, { useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';

interface CameraInfo {
  camera_make?: string | null;
  camera_model?: string | null;
  lens_model?: string | null;
  iso?: number | null;
  aperture?: string | null;
  shutter_speed?: string | null;
  focal_length?: string | null;
  telescope?: string | null;
  mount?: string | null;
  filters?: string | null;
  total_exposure_time?: number | null;
  ra?: string | null;
  dec?: string | null;
}

interface HoverImageInfo {
  name: string;
  title?: string | null;
  description?: string | null;
  capture_date?: string | null;
  camera_info?: CameraInfo | null;
}

const HoverCard: React.FC<{ gallery: string; path: string; anchor: HTMLElement }> = ({
  gallery,
  path,
  anchor,
}) => {
  const [visible, setVisible] = useState(false);
  const [info, setInfo] = useState<HoverImageInfo | null>(null);
  const [failed, setFailed] = useState(false);
  const fetched = useRef(false);

  useEffect(() => {
    const show = () => {
      setVisible(true);
      if (!fetched.current) {
        fetched.current = true;
        fetch(`/api/gallery/${encodeURIComponent(gallery)}/image/${encodeURIComponent(path)}`)
          .then((response) => {
            if (!response.ok) throw new Error(`${response.status}`);
            return response.json();
          })
          .then((data: { image: HoverImageInfo }) => setInfo(data.image))
          .catch(() => setFailed(true));
      }
    };
    const hide = () => setVisible(false);

    anchor.addEventListener('mouseenter', show);
    anchor.addEventListener('mouseleave', hide);
    anchor.addEventListener('focus', show);
    anchor.addEventListener('blur', hide);
    return () => {
      anchor.removeEventListener('mouseenter', show);
      anchor.removeEventListener('mouseleave', hide);
      anchor.removeEventListener('focus', show);
      anchor.removeEventListener('blur', hide);
    };
  }, [anchor, gallery, path]);

  if (!visible || failed) return null;

  const camera = info?.camera_info;
  const exposure = camera
    ? [
        camera.shutter_speed,
        camera.aperture,
        camera.iso ? `ISO ${camera.iso}` : null,
        camera.focal_length,
      ]
        .filter(Boolean)
        .join(' · ')
    : '';
  const cameraName = camera
    ? [camera.camera_make, camera.camera_model].filter(Boolean).join(' ')
    : '';

  return (
    <div className="gallery-hover-card" role="tooltip">
      {!info && <p className="gallery-hover-loading">Loading…</p>}
      {info && (
        <>
          <p className="gallery-hover-title">{info.title || info.name}</p>
          {info.description && (
            <div
              className="gallery-hover-description"
              dangerouslySetInnerHTML={{ __html: info.description }}
            />
          )}
          <dl className="gallery-hover-specs">
            {cameraName && (
              <>
                <dt>Camera</dt>
                <dd>{cameraName}</dd>
              </>
            )}
            {camera?.lens_model && (
              <>
                <dt>Lens</dt>
                <dd>{camera.lens_model}</dd>
              </>
            )}
            {exposure && (
              <>
                <dt>Exposure</dt>
                <dd>{exposure}</dd>
              </>
            )}
            {camera?.telescope && (
              <>
                <dt>Telescope</dt>
                <dd>{camera.telescope}</dd>
              </>
            )}
            {camera?.mount && (
              <>
                <dt>Mount</dt>
                <dd>{camera.mount}</dd>
              </>
            )}
            {camera?.filters && (
              <>
                <dt>Filters</dt>
                <dd>{camera.filters}</dd>
              </>
            )}
            {camera?.total_exposure_time != null && (
              <>
                <dt>Integration</dt>
                <dd>{camera.total_exposure_time} h</dd>
              </>
            )}
            {camera?.ra && camera?.dec && (
              <>
                <dt>RA / Dec</dt>
                <dd>
                  {camera.ra} / {camera.dec}
                </dd>
              </>
            )}
            {info.capture_date && (
              <>
                <dt>Captured</dt>
                <dd>{info.capture_date}</dd>
              </>
            )}
          </dl>
        </>
      )}
    </div>
  );
};

document.addEventListener('DOMContentLoaded', () => {
  document.querySelectorAll<HTMLElement>('.gallery-image-details').forEach((anchor) => {
    const gallery = anchor.dataset.gallery;
    const path = anchor.dataset.imagePath;
    if (!gallery || !path) return;

    const mount = document.createElement('span');
    mount.className = 'gallery-hover-mount';
    anchor.appendChild(mount);
    createRoot(mount).render(<HoverCard gallery={gallery} path={path} anchor={anchor} />);
  });
});
