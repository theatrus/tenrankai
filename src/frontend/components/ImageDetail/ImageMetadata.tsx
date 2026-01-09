import { ImageInfo, RolePermissions } from '../../types/index.ts';

interface ImageMetadataProps {
  image: ImageInfo;
  hideMetadata?: boolean;
  permissions: RolePermissions;
}

export function ImageMetadata({ image, hideMetadata, permissions }: ImageMetadataProps) {
  if (hideMetadata) {
    return null;
  }

  const totalPixels = image.dimensions[0] * image.dimensions[1];
  const megapixels = Math.round(totalPixels / 1000000);
  const fileSizeMB = (image.file_size / 1024 / 1024).toFixed(2);

  return (
    <div className="image-metadata card">
      <h3>Image Information</h3>
      <dl>
        <dt>Filename</dt>
        <dd>{image.name}</dd>
        
        {image.capture_date && (
          <>
            <dt>Capture Date</dt>
            <dd>{image.capture_date}</dd>
          </>
        )}
        
        <dt>Dimensions</dt>
        <dd>
          {image.dimensions[0]} × {image.dimensions[1]} pixels
          {megapixels > 0 && ` (${megapixels} MP)`}
        </dd>
        
        <dt>File Size</dt>
        <dd>{fileSizeMB} MB</dd>
        
        {image.color_profile && permissions.can_see_technical_details && (
          <>
            <dt>Color Profile</dt>
            <dd>{image.color_profile}</dd>
          </>
        )}
      </dl>
    </div>
  );
}

export function CameraMetadata({ image, permissions }: { image: ImageInfo; permissions: RolePermissions }) {
  if (!image.camera_info || !permissions.can_see_technical_details) {
    return null;
  }

  const camera = image.camera_info;
  const cameraName = [
    camera.camera_make,
    camera.camera_model
  ].filter(Boolean).join(' ');
  
  // Check if this is an astronomical image
  const isAstronomical = camera.telescope || camera.mount || camera.filters || 
                        camera.total_exposure_time || camera.ra || camera.dec;

  return (
    <div className="camera-info card">
      <h3>{isAstronomical ? 'Technical Information' : 'Camera Information'}</h3>
      <dl>
        {cameraName && (
          <>
            <dt>Camera</dt>
            <dd>{cameraName}</dd>
          </>
        )}
        
        {camera.lens_model && (
          <>
            <dt>Lens</dt>
            <dd>{camera.lens_model}</dd>
          </>
        )}
        
        {camera.telescope && (
          <>
            <dt>Telescope</dt>
            <dd>{camera.telescope}</dd>
          </>
        )}
        
        {camera.mount && (
          <>
            <dt>Mount</dt>
            <dd>{camera.mount}</dd>
          </>
        )}
        
        {camera.filters && (
          <>
            <dt>Filters</dt>
            <dd>{camera.filters}</dd>
          </>
        )}
        
        {camera.focal_length && (
          <>
            <dt>Focal Length</dt>
            <dd>{camera.focal_length}</dd>
          </>
        )}
        
        {camera.aperture && (
          <>
            <dt>Aperture</dt>
            <dd>{camera.aperture}</dd>
          </>
        )}
        
        {camera.shutter_speed && (
          <>
            <dt>Shutter Speed</dt>
            <dd>{camera.shutter_speed}</dd>
          </>
        )}
        
        {camera.total_exposure_time && (
          <>
            <dt>Total Exposure Time</dt>
            <dd>{camera.total_exposure_time} hours</dd>
          </>
        )}
        
        {camera.iso && (
          <>
            <dt>ISO</dt>
            <dd>{camera.iso}</dd>
          </>
        )}
        
        {camera.ra && (
          <>
            <dt>Right Ascension</dt>
            <dd>{camera.ra}</dd>
          </>
        )}
        
        {camera.dec && (
          <>
            <dt>Declination</dt>
            <dd>{camera.dec}</dd>
          </>
        )}
        
        {camera.additional_details && (
          <>
            <dt>Additional Details</dt>
            <dd>{camera.additional_details}</dd>
          </>
        )}
      </dl>
    </div>
  );
}

export function LocationMetadata({ image, permissions }: { image: ImageInfo; permissions: RolePermissions }) {
  if (!image.location_info || !permissions.can_see_location) {
    return null;
  }

  const location = image.location_info;

  return (
    <div className="location-info card">
      <h3>Location</h3>
      <div className="location-content">
        <div className="coordinates">
          <span className="coord-label">Coordinates:</span>
          <span className="coordinates-text">
            {location.latitude.toFixed(6)}, {location.longitude.toFixed(6)}
          </span>
        </div>
        
        <div className="map-links">
          <a href={location.google_maps_url} target="_blank" rel="noopener noreferrer" className="map-link google-maps">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
              <path d="M21,3V5H19V3H21M19,7H21V9H19V7M19,11H21V13H19V11M19,15H21V17H19V15M19,19H21V21H19V19M17,21V19H15V21H17M13,21V19H11V21H13M9,21V19H7V21H9M5,21V19H3V21H5M3,17V15H5V17H3M3,13V11H5V13H3M3,9V7H5V9H3M3,5V3H5V5H3M7,5V3H9V5H7M11,5V3H13V5H11M15,5V3H17V5H15"/>
            </svg>
            Google Maps
          </a>
          <a href={location.apple_maps_url} target="_blank" rel="noopener noreferrer" className="map-link apple-maps">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
              <path d="M12,11.5A2.5,2.5 0 0,1 9.5,9A2.5,2.5 0 0,1 12,6.5A2.5,2.5 0 0,1 14.5,9A2.5,2.5 0 0,1 12,11.5M12,2A7,7 0 0,0 5,9C5,14.25 12,22 12,22C12,22 19,14.25 19,9A7,7 0 0,0 12,2Z"/>
            </svg>
            Apple Maps
          </a>
        </div>
        
        <div className="embedded-map">
          <iframe 
            src={`https://www.openstreetmap.org/export/embed.html?bbox=${location.longitude - 0.01}%2C${location.latitude - 0.01}%2C${location.longitude + 0.01}%2C${location.latitude + 0.01}&layer=mapnik&marker=${location.latitude}%2C${location.longitude}`}
            width="100%"
            height="200"
            style={{ border: 0 }}
            loading="lazy"
            title="Location Map"
          />
        </div>
      </div>
    </div>
  );
}