import { NavigationImage } from '../../types/index.ts';

interface MobileNavigationProps {
  prevImage?: NavigationImage;
  nextImage?: NavigationImage;
  onNavigate?: (direction: 'prev' | 'next') => void;
}

export function MobileNavigation({ prevImage, nextImage, onNavigate }: MobileNavigationProps) {
  if (!prevImage && !nextImage) {
    return null;
  }

  return (
    <div className="mobile-navigation">
      <button 
        className="mobile-nav-btn mobile-nav-prev" 
        onClick={() => onNavigate?.('prev')}
        disabled={!prevImage}
        aria-label="Previous image"
      >
        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3">
          <polyline points="15,18 9,12 15,6"></polyline>
        </svg>
      </button>

      <button 
        className="mobile-nav-btn mobile-nav-next" 
        onClick={() => onNavigate?.('next')}
        disabled={!nextImage}
        aria-label="Next image"
      >
        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3">
          <polyline points="9,6 15,12 9,18"></polyline>
        </svg>
      </button>
    </div>
  );
}