import React, { useState, useCallback, useRef } from 'react';
import { galleryManageApi } from '../../api/gallery-manage';
import type { SortOrder, SortDirection } from '../../types';

interface SortOrderControlProps {
  galleryName: string;
  folderPath: string;
  currentSortOrder: SortOrder;
  currentSortDirection: SortDirection;
  images: { path: string; name: string; thumbnail_url?: string }[];
  onSortChanged: () => void;
}

const SORT_ORDER_LABELS: Record<SortOrder, string> = {
  capture_time: 'Capture Time',
  filename: 'Filename',
  custom: 'Custom',
};

const SORT_DIRECTION_LABELS: Record<SortDirection, string> = {
  asc: 'Ascending',
  desc: 'Descending',
};

export const SortOrderControl: React.FC<SortOrderControlProps> = ({
  galleryName,
  folderPath,
  currentSortOrder,
  currentSortDirection,
  images,
  onSortChanged,
}) => {
  const [saving, setSaving] = useState(false);
  const [customOrderMode, setCustomOrderMode] = useState(false);
  const [dragOrder, setDragOrder] = useState<string[]>([]);
  const [hasChanges, setHasChanges] = useState(false);
  const dragItemRef = useRef<number | null>(null);
  const dragOverItemRef = useRef<number | null>(null);

  const handleSortOrderChange = useCallback(async (newOrder: SortOrder) => {
    if (newOrder === currentSortOrder) return;

    if (newOrder === 'custom') {
      const filenames = images
        .filter(img => !img.path.includes('/') || img.path.split('/').length <= 2)
        .map(img => img.name);
      setDragOrder(filenames);
      setCustomOrderMode(true);
      setHasChanges(false);
      return;
    }

    setCustomOrderMode(false);
    setSaving(true);
    try {
      await galleryManageApi.updateSortOrder(galleryName, folderPath, {
        sort_order: newOrder,
        sort_direction: currentSortDirection,
      });
      onSortChanged();
    } catch (err) {
      alert(`Failed to update sort order: ${err instanceof Error ? err.message : 'Unknown error'}`);
    } finally {
      setSaving(false);
    }
  }, [galleryName, folderPath, currentSortOrder, currentSortDirection, images, onSortChanged]);

  const handleDirectionChange = useCallback(async (newDirection: SortDirection) => {
    if (newDirection === currentSortDirection && !customOrderMode) return;

    if (customOrderMode) {
      return;
    }

    setSaving(true);
    try {
      await galleryManageApi.updateSortOrder(galleryName, folderPath, {
        sort_order: currentSortOrder,
        sort_direction: newDirection,
      });
      onSortChanged();
    } catch (err) {
      alert(`Failed to update sort direction: ${err instanceof Error ? err.message : 'Unknown error'}`);
    } finally {
      setSaving(false);
    }
  }, [galleryName, folderPath, currentSortOrder, currentSortDirection, customOrderMode, onSortChanged]);

  const handleDragStart = useCallback((index: number) => {
    dragItemRef.current = index;
  }, []);

  const handleDragEnter = useCallback((index: number) => {
    dragOverItemRef.current = index;
  }, []);

  const handleDragEnd = useCallback(() => {
    if (dragItemRef.current === null || dragOverItemRef.current === null) return;
    if (dragItemRef.current === dragOverItemRef.current) {
      dragItemRef.current = null;
      dragOverItemRef.current = null;
      return;
    }

    setDragOrder(prev => {
      const updated = [...prev];
      const [removed] = updated.splice(dragItemRef.current!, 1);
      updated.splice(dragOverItemRef.current!, 0, removed);
      return updated;
    });
    setHasChanges(true);
    dragItemRef.current = null;
    dragOverItemRef.current = null;
  }, []);

  const handleSaveCustomOrder = useCallback(async () => {
    setSaving(true);
    try {
      await galleryManageApi.updateSortOrder(galleryName, folderPath, {
        sort_order: 'custom',
        sort_direction: currentSortDirection,
        custom_order: dragOrder,
      });
      setCustomOrderMode(false);
      setHasChanges(false);
      onSortChanged();
    } catch (err) {
      alert(`Failed to save custom order: ${err instanceof Error ? err.message : 'Unknown error'}`);
    } finally {
      setSaving(false);
    }
  }, [galleryName, folderPath, currentSortDirection, dragOrder, onSortChanged]);

  const handleResetCustomOrder = useCallback(() => {
    const filenames = images.map(img => img.name);
    setDragOrder(filenames);
    setHasChanges(false);
  }, [images]);

  const handleCancelCustom = useCallback(() => {
    setCustomOrderMode(false);
    setHasChanges(false);
  }, []);

  const imagesByName = React.useMemo(() => {
    const map = new Map<string, string>();
    for (const img of images) {
      map.set(img.name, img.thumbnail_url || '');
    }
    return map;
  }, [images]);

  return (
    <div className="sort-order-control">
      <div className="sort-order-selectors">
        <label className="sort-order-label">
          Sort:
          <select
            className="sort-order-select"
            value={customOrderMode ? 'custom' : currentSortOrder}
            onChange={(e) => handleSortOrderChange(e.target.value as SortOrder)}
            disabled={saving}
          >
            {Object.entries(SORT_ORDER_LABELS).map(([value, label]) => (
              <option key={value} value={value}>{label}</option>
            ))}
          </select>
        </label>
        <label className="sort-order-label">
          Direction:
          <select
            className="sort-order-select"
            value={currentSortDirection}
            onChange={(e) => handleDirectionChange(e.target.value as SortDirection)}
            disabled={saving || customOrderMode}
          >
            {Object.entries(SORT_DIRECTION_LABELS).map(([value, label]) => (
              <option key={value} value={value}>{label}</option>
            ))}
          </select>
        </label>
        {saving && <span className="sort-order-saving">Saving...</span>}
      </div>

      {customOrderMode && (
        <div className="sort-order-custom">
          <div className="sort-order-custom-header">
            <span>Drag images to reorder</span>
            <div className="sort-order-custom-actions">
              <button
                className="btn btn-primary btn-sm"
                onClick={handleSaveCustomOrder}
                disabled={saving || !hasChanges}
              >
                {saving ? 'Saving...' : 'Save Order'}
              </button>
              <button
                className="btn btn-secondary btn-sm"
                onClick={handleResetCustomOrder}
                disabled={saving}
              >
                Reset
              </button>
              <button
                className="btn btn-secondary btn-sm"
                onClick={handleCancelCustom}
                disabled={saving}
              >
                Cancel
              </button>
            </div>
          </div>
          <div className="sort-order-drag-grid">
            {dragOrder.map((filename, index) => (
              <div
                key={filename}
                className="sort-order-drag-item"
                draggable
                onDragStart={() => handleDragStart(index)}
                onDragEnter={() => handleDragEnter(index)}
                onDragEnd={handleDragEnd}
                onDragOver={(e) => e.preventDefault()}
              >
                <div
                  className="sort-order-drag-thumb"
                  style={{
                    backgroundImage: imagesByName.get(filename)
                      ? `url("${imagesByName.get(filename)}")`
                      : undefined,
                  }}
                />
                <span className="sort-order-drag-label">{filename}</span>
                <span className="sort-order-drag-index">{index + 1}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
};
