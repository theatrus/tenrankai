import { NavigationImage } from '../../types/index.ts';

interface ImageNavigationProps {
  prevImage?: NavigationImage;
  nextImage?: NavigationImage;
  galleryUrl: string;
  onNavigate?: (direction: 'prev' | 'next') => void;
  title?: string;
  description?: string;
}

export function ImageNavigation({ prevImage, nextImage, galleryUrl, onNavigate, title, description }: ImageNavigationProps) {
  if (!prevImage && !nextImage) {
    return null;
  }

  const handlePrevClick = () => {
    if (onNavigate) {
      onNavigate('prev');
    } else {
      window.location.href = `${galleryUrl}/detail/${prevImage!.path}`;
    }
  };

  const handleNextClick = () => {
    if (onNavigate) {
      onNavigate('next');
    } else {
      window.location.href = `${galleryUrl}/detail/${nextImage!.path}`;
    }
  };

  return (
    <div className="image-navigation">
      {prevImage ? (
        <button 
          className="nav-item nav-prev" 
          onClick={handlePrevClick}
          title={`Previous: ${prevImage.name}`}
        >
          <div className="nav-thumbnail">
            <img 
              src={prevImage.thumbnail_url} 
              srcSet={`${prevImage.thumbnail_url} 1x, ${prevImage.thumbnail_url.replace('?size=thumbnail', '?size=thumbnail@2x')} 2x`}
              alt={prevImage.name}
            />
          </div>
          <div className="nav-info">
            <div className="nav-direction">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <polyline points="15,18 9,12 15,6"></polyline>
              </svg>
              Previous
            </div>
            <div className="nav-filename">{prevImage.name}</div>
          </div>
        </button>
      ) : (
        <div className="nav-spacer"></div>
      )}
      
      {/* Image title and description - desktop only */}
      {(title || description) && (
        <div className="nav-image-info hide-mobile">
          {title && <h2 className="nav-image-title">{title}</h2>}
          {description && (
            <div 
              className="nav-image-description"
              dangerouslySetInnerHTML={{ __html: description }}
            />
          )}
        </div>
      )}
      
      {nextImage ? (
        <button 
          className="nav-item nav-next" 
          onClick={handleNextClick}
          title={`Next: ${nextImage.name}`}
        >
          <div className="nav-info">
            <div className="nav-direction">
              Next
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <polyline points="9,6 15,12 9,18"></polyline>
              </svg>
            </div>
            <div className="nav-filename">{nextImage.name}</div>
          </div>
          <div className="nav-thumbnail">
            <img 
              src={nextImage.thumbnail_url}
              srcSet={`${nextImage.thumbnail_url} 1x, ${nextImage.thumbnail_url.replace('?size=thumbnail', '?size=thumbnail@2x')} 2x`}
              alt={nextImage.name}
            />
          </div>
        </button>
      ) : (
        <div className="nav-spacer"></div>
      )}
    </div>
  );
}