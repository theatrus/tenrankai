import React, { useState, useCallback } from 'react';
import { createPortal } from 'react-dom';
import { MasonryGrid } from './MasonryGrid';
import { FilterBar, FilterType } from './FilterBar';
import { ManageToolbar } from './ManageToolbar';
import type { GalleryImage, GridMode } from './MasonryGrid';
import type { RolePermissions } from '../../types';

interface GalleryWithFilterProps {
  images: GalleryImage[];
  galleryUrl: string;
  permissions?: RolePermissions;
  filterMount?: HTMLElement | null;
  galleryName: string;
  galleryPath: string;
  hiddenImages: string[];
  isManageMode: boolean;
  selectedImages: Set<string>;
  onToggleSelect: (path: string) => void;
  onHideSuccess: (hiddenImages: string[]) => void;
  onDeleteSuccess: (deletedPaths: string[]) => void;
  onMoveSuccess: (movedCount: number) => void;
  onCopySuccess: (copiedCount: number) => void;
  onCancelManage: () => void;
  toolbarMount?: HTMLElement | null;
  gridMode?: GridMode;
  maxColumns?: number;
}

export const GalleryWithFilter: React.FC<GalleryWithFilterProps> = ({
  images,
  galleryUrl,
  permissions,
  filterMount,
  galleryName,
  galleryPath,
  hiddenImages,
  isManageMode,
  selectedImages,
  onToggleSelect,
  onHideSuccess,
  onDeleteSuccess,
  onMoveSuccess,
  onCopySuccess,
  onCancelManage,
  toolbarMount,
  gridMode = 'masonry',
  maxColumns,
}) => {
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

  const handleFilterChange = useCallback((filter: FilterType) => {
    setActiveFilter(filter);

    if (typeof window !== 'undefined') {
      const url = new URL(window.location.href);
      if (filter === 'all') {
        url.searchParams.delete('filter');
      } else {
        url.searchParams.set('filter', filter);
      }
      window.history.pushState({}, '', url.toString());
    }
  }, []);

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

  const filterRootRef = React.useRef<any>(null);

  React.useEffect(() => {
    if (filterMount && permissions?.can_read_metadata && !filterRootRef.current) {
      import('react-dom/client').then(({ createRoot }) => {
        filterRootRef.current = createRoot(filterMount);
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
      if (filterRootRef.current) {
        setTimeout(() => {
          filterRootRef.current?.unmount();
          filterRootRef.current = null;
        }, 0);
      }
    };
  }, [filterMount, permissions?.can_read_metadata]);

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
    <>
      <MasonryGrid
        images={filteredImages}
        galleryUrl={galleryUrl}
        permissions={permissions}
        isManageMode={isManageMode}
        selectedImages={selectedImages}
        hiddenImages={hiddenImages}
        onToggleSelect={onToggleSelect}
        gridMode={gridMode}
        maxColumns={maxColumns}
      />
      {isManageMode && toolbarMount && createPortal(
        <ManageToolbar
          galleryName={galleryName}
          galleryPath={galleryPath}
          selectedImages={selectedImages}
          onHideSuccess={onHideSuccess}
          onDeleteSuccess={onDeleteSuccess}
          onMoveSuccess={onMoveSuccess}
          onCopySuccess={onCopySuccess}
          onCancel={onCancelManage}
        />,
        toolbarMount
      )}
    </>
  );
};
