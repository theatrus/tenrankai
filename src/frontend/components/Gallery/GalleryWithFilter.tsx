import React, { useState, useCallback } from 'react';
import { MasonryGrid } from './MasonryGrid';
import { FilterBar, FilterType } from './FilterBar';
import type { GalleryImage } from './MasonryGrid';
import type { RolePermissions } from '../../types';

interface GalleryWithFilterProps {
  images: GalleryImage[];
  galleryUrl: string;
  permissions?: RolePermissions;
  filterMount?: HTMLElement | null;
}

export const GalleryWithFilter: React.FC<GalleryWithFilterProps> = ({ 
  images, 
  galleryUrl, 
  permissions,
  filterMount 
}) => {
  // Initialize filter from URL parameters
  const getInitialFilter = (): FilterType => {
    if (typeof window !== 'undefined') {
      const params = new URLSearchParams(window.location.search);
      const filter = params.get('filter');
      if (filter && ['all', 'picks', 'rejects', 'highlighted', 'commented'].includes(filter)) {
        return filter as FilterType;
      }
    }
    return 'all';
  };

  const [activeFilter, setActiveFilter] = useState<FilterType>(getInitialFilter());

  // Update URL when filter changes
  const handleFilterChange = useCallback((filter: FilterType) => {
    setActiveFilter(filter);
    
    if (typeof window !== 'undefined') {
      const url = new URL(window.location.href);
      if (filter === 'all') {
        url.searchParams.delete('filter');
      } else {
        url.searchParams.set('filter', filter);
      }
      // Update URL without page reload
      window.history.pushState({}, '', url.toString());
    }
  }, []);

  // Calculate filter counts
  const filterCounts = React.useMemo(() => {
    const counts = {
      all: images.length,
      picks: 0,
      rejects: 0,
      highlighted: 0,
      commented: 0,
    };

    if (!permissions?.can_read_metadata) {
      return counts;
    }

    images.forEach((image) => {
      const metadata = image.user_metadata;
      if (!metadata) return;

      if (metadata.pick_status === 'pick') counts.picks++;
      if (metadata.pick_status === 'no_pick') counts.rejects++;
      if (metadata.highlighted) counts.highlighted++;
      if (metadata.comments && metadata.comments.length > 0) counts.commented++;
    });

    return counts;
  }, [images, permissions]);

  // Filter images based on active filter
  const filteredImages = React.useMemo(() => {
    if (!permissions?.can_read_metadata || activeFilter === 'all') {
      return images;
    }

    return images.filter((image) => {
      const metadata = image.user_metadata;
      if (!metadata) return false;

      switch (activeFilter) {
        case 'picks':
          return metadata.pick_status === 'pick';
        case 'rejects':
          return metadata.pick_status === 'no_pick';
        case 'highlighted':
          return metadata.highlighted === true;
        case 'commented':
          return metadata.comments && metadata.comments.length > 0;
        default:
          return true;
      }
    });
  }, [images, activeFilter, permissions]);

  // Create a ref to store the filter root
  const filterRootRef = React.useRef<any>(null);
  
  // Mount filter bar only once
  React.useEffect(() => {
    if (filterMount && permissions?.can_read_metadata && !filterRootRef.current) {
      import('react-dom/client').then(({ createRoot }) => {
        filterRootRef.current = createRoot(filterMount);
        // Initial render
        filterRootRef.current.render(
          <FilterBar
            activeFilter={activeFilter}
            onFilterChange={handleFilterChange}
            counts={filterCounts}
          />
        );
      });
    }
    
    return () => {
      // Cleanup on unmount
      if (filterRootRef.current) {
        setTimeout(() => {
          filterRootRef.current?.unmount();
          filterRootRef.current = null;
        }, 0);
      }
    };
  }, [filterMount, permissions?.can_read_metadata]); // Note: intentionally not including other deps to prevent remount
  
  // Update filter bar when props change
  React.useEffect(() => {
    if (filterRootRef.current && permissions?.can_read_metadata) {
      filterRootRef.current.render(
        <FilterBar
          activeFilter={activeFilter}
          onFilterChange={handleFilterChange}
          counts={filterCounts}
        />
      );
    }
  }, [activeFilter, filterCounts, handleFilterChange, permissions?.can_read_metadata]);

  return (
    <MasonryGrid 
      images={filteredImages}
      galleryUrl={galleryUrl}
      permissions={permissions}
    />
  );
};