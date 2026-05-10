import { NavigationImage } from '../../types/index.ts';

interface ImageNavigationProps {
  prevImage?: NavigationImage;
  nextImage?: NavigationImage;
  prevImages: NavigationImage[];
  nextImages: NavigationImage[];
  galleryUrl: string;
  onNavigate?: (direction: 'prev' | 'next') => void;
  onNavigateToImage?: (image: NavigationImage) => void;
  title?: string;
  canEditContent?: boolean;
  onEditClick?: () => void;
}

export function ImageNavigation({
  prevImage,
  nextImage,
  prevImages,
  nextImages,
  galleryUrl,
  onNavigate,
  onNavigateToImage,
  title,
  canEditContent,
  onEditClick
}: ImageNavigationProps) {
  if (!prevImage && !nextImage && prevImages.length === 0 && nextImages.length === 0) {
    return null;
  }

  const handleImageClick = (image: NavigationImage, direction: 'prev' | 'next') => {
    if (onNavigateToImage) {
      onNavigateToImage(image);
    } else if (onNavigate && ((direction === 'prev' && image === prevImage) || (direction === 'next' && image === nextImage))) {
      onNavigate(direction);
    } else {
      window.location.href = `${galleryUrl}/detail/${image.path}`;
    }
  };

  const handlePrevClick = () => {
    if (prevImage) {
      handleImageClick(prevImage, 'prev');
    }
  };

  const handleNextClick = () => {
    if (nextImage) {
      handleImageClick(nextImage, 'next');
    }
  };

  // Render a single thumbnail in the strip
  const renderThumbnail = (
    image: NavigationImage,
    index: number,
    direction: 'prev' | 'next',
    isImmediate: boolean
  ) => {
    const thumbnailUrl = image.thumbnail_url;
    const thumbnail2xUrl = thumbnailUrl.replace(/\/thumbnail$/, '/thumbnail@2x');

    return (
      <button
        key={`${direction}-${index}`}
        className={`nav-strip-thumb ${isImmediate ? 'nav-strip-thumb-immediate' : ''}`}
        onClick={() => handleImageClick(image, direction)}
        title={image.name}
        aria-label={isImmediate
          ? `${direction === 'prev' ? 'Previous' : 'Next'}: ${image.name}`
          : `Go to ${image.name}`
        }
      >
        <img
          src={thumbnailUrl}
          srcSet={`${thumbnailUrl} 1x, ${thumbnail2xUrl} 2x`}
          alt={image.name}
          loading="lazy"
        />
      </button>
    );
  };

  // For prev side: show images in reverse order (furthest first, closest last)
  // prevImages is already closest-first from backend, so we reverse for display
  const prevThumbnails = [...prevImages].reverse();

  // For next side: show images in order (closest first, furthest last)
  const nextThumbnails = nextImages;

  const hasPrev = prevThumbnails.length > 0;
  const hasNext = nextThumbnails.length > 0;

  return (
    <div className="image-navigation">
      {/* Previous images strip */}
      <div className="nav-strip nav-strip-prev">
        {hasPrev && <div className="nav-strip-fade nav-strip-fade-left"></div>}
        <div className="nav-strip-container">
          {prevThumbnails.map((img, idx) =>
            renderThumbnail(img, idx, 'prev', img === prevImage)
          )}
        </div>
        {prevImage && (
          <button
            className="nav-strip-arrow nav-strip-arrow-prev"
            onClick={handlePrevClick}
            title={`Previous: ${prevImage.name}`}
            aria-label={`Previous: ${prevImage.name}`}
          >
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <polyline points="15,18 9,12 15,6"></polyline>
            </svg>
          </button>
        )}
      </div>

      {/* Image title - desktop only */}
      {title ? (
        <div className="nav-image-info hide-mobile">
          <div className="image-title-row">
            <h2 className="nav-image-title">{title}</h2>
            {canEditContent && onEditClick && (
              <button
                type="button"
                className="image-edit-icon"
                onClick={onEditClick}
                title="Edit image info"
                aria-label="Edit image info"
              >
                <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                  <path d="M12.146.146a.5.5 0 0 1 .708 0l3 3a.5.5 0 0 1 0 .708l-10 10a.5.5 0 0 1-.168.11l-5 2a.5.5 0 0 1-.65-.65l2-5a.5.5 0 0 1 .11-.168l10-10zM11.207 2.5 13.5 4.793 14.793 3.5 12.5 1.207 11.207 2.5zm1.586 3L10.5 3.207 4 9.707V10h.5a.5.5 0 0 1 .5.5v.5h.5a.5.5 0 0 1 .5.5v.5h.293l6.5-6.5zm-9.761 5.175-.106.106-1.528 3.821 3.821-1.528.106-.106A.5.5 0 0 1 5 12.5V12h-.5a.5.5 0 0 1-.5-.5V11h-.5a.5.5 0 0 1-.468-.325z"/>
                </svg>
              </button>
            )}
          </div>
        </div>
      ) : (
        <div className="nav-spacer"></div>
      )}

      {/* Next images strip */}
      <div className="nav-strip nav-strip-next">
        {nextImage && (
          <button
            className="nav-strip-arrow nav-strip-arrow-next"
            onClick={handleNextClick}
            title={`Next: ${nextImage.name}`}
            aria-label={`Next: ${nextImage.name}`}
          >
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <polyline points="9,6 15,12 9,18"></polyline>
            </svg>
          </button>
        )}
        <div className="nav-strip-container">
          {nextThumbnails.map((img, idx) =>
            renderThumbnail(img, idx, 'next', img === nextImage)
          )}
        </div>
        {hasNext && <div className="nav-strip-fade nav-strip-fade-right"></div>}
      </div>
    </div>
  );
}
