import { useEffect } from 'react';
import { NavigationImage } from '../types/index.ts';

interface UseKeyboardNavigationOptions {
  prevImage?: NavigationImage;
  nextImage?: NavigationImage;
  galleryUrl: string;
  imagePath: string;
  onNavigate?: (direction: 'prev' | 'next' | 'back') => void;
}

export function useKeyboardNavigation({
  prevImage,
  nextImage,
  galleryUrl,
  imagePath,
  onNavigate
}: UseKeyboardNavigationOptions) {
  useEffect(() => {
    const handleKeydown = (event: KeyboardEvent) => {
      // Don't interfere when user is typing in an input
      if (event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement) {
        return;
      }

      switch (event.key) {
        case 'ArrowLeft':
          if (prevImage) {
            event.preventDefault();
            if (onNavigate) {
              onNavigate('prev');
            } else {
              window.location.href = `${galleryUrl}/detail/${prevImage.path}`;
            }
          }
          break;
          
        case 'ArrowRight':
          if (nextImage) {
            event.preventDefault();
            if (onNavigate) {
              onNavigate('next');
            } else {
              window.location.href = `${galleryUrl}/detail/${nextImage.path}`;
            }
          }
          break;
          
        case 'Escape':
          event.preventDefault();
          if (onNavigate) {
            onNavigate('back');
          } else {
            // Navigate back to gallery
            const pathParts = imagePath.split('/');
            const galleryPath = pathParts.slice(0, -1).join('/');
            window.location.href = `${galleryUrl}/${galleryPath}`;
          }
          break;
      }
    };

    document.addEventListener('keydown', handleKeydown);
    return () => document.removeEventListener('keydown', handleKeydown);
  }, [prevImage, nextImage, galleryUrl, imagePath, onNavigate]);
}