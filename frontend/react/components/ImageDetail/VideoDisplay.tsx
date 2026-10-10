import { useEffect, useState } from 'react';
import type { ImageInfo } from '../../types';

export function formatVideoDuration(seconds?: number): string {
  if (seconds === undefined || !Number.isFinite(seconds)) return '';
  const total = Math.max(0, Math.round(seconds));
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`;
}

export function VideoDisplay({ image, onRefresh }: { image: ImageInfo; onRefresh?: () => void }) {
  const [playbackError, setPlaybackError] = useState(false);
  const [playbackAttempt, setPlaybackAttempt] = useState(0);
  useEffect(() => setPlaybackError(false), [image.path, image.video?.playback_url]);
  const video = image.video;
  const ready = video?.state === 'ready' && video.playback_url;
  const failed = video?.state === 'failed';

  return (
    <div className="video-display" data-testid="video-display">
      {ready ? (
        <video
          key={`${image.path}:${video.playback_url}:${playbackAttempt}`}
          src={video.playback_url}
          poster={video.poster_url || image.medium_url}
          controls
          playsInline
          preload="metadata"
          aria-label={image.title || image.name}
          onError={() => setPlaybackError(true)}
        >
          Your browser does not support video playback.
        </video>
      ) : (
        <div className="video-status" role="status" aria-live="polite">
          {video?.poster_url && <img src={video.poster_url} alt="" />}
          <span className="video-status-icon" aria-hidden="true">▶</span>
          <p>{failed ? 'This video could not be prepared for playback.' : 'Preparing video for playback…'}</p>
          {!failed && <p className="video-status-hint">You can leave this page and come back when it’s ready.</p>}
        </div>
      )}
      {playbackError && (
        <div role="alert" className="video-playback-error">
          <p>The video could not be played.</p>
          <button className="btn" onClick={() => { setPlaybackError(false); setPlaybackAttempt(attempt => attempt + 1); onRefresh?.(); }}>Try again</button>
        </div>
      )}
    </div>
  );
}
