import React from 'react';

export type FilterType = 'all' | 'picks' | 'rejects' | 'highlighted' | 'commented' | 'photos' | 'videos';

interface FilterBarProps {
  activeFilter: FilterType;
  onFilterChange: (filter: FilterType) => void;
  canReadMetadata?: boolean;
  counts: {
    photos?: number;
    videos?: number;
    all: number;
    picks: number;
    rejects: number;
    highlighted: number;
    commented: number;
  };
}

export const FilterBar: React.FC<FilterBarProps> = ({ activeFilter, onFilterChange, counts, canReadMetadata = true }) => {
  const filters: { type: FilterType; label: string; icon?: string }[] = [
    { type: 'all', label: 'All' },
    ...((counts.videos || 0) > 0 ? [{ type: 'photos' as const, label: 'Photos' }, { type: 'videos' as const, label: 'Videos', icon: '▶' }] : []),
    { type: 'picks', label: 'Picks', icon: '✓' },
    { type: 'rejects', label: 'Rejects', icon: '✗' },
    { type: 'highlighted', label: 'Starred', icon: '⭐' },
    { type: 'commented', label: 'Comments', icon: '💬' },
  ];

  return (
    <div className="gallery-filter-bar">
      <div className="filter-buttons">
        {filters.map((filter) => {
          const count = counts[filter.type] || 0;
          if (!['all', 'photos', 'videos'].includes(filter.type) && !canReadMetadata) return null;
          const isActive = activeFilter === filter.type;
          
          return (
            <button
              key={filter.type}
              className={`filter-btn ${isActive ? 'active' : ''} ${count === 0 ? 'disabled' : ''} ${['photos', 'videos'].includes(filter.type) ? 'media-filter' : ''}`}
              onClick={() => count > 0 && onFilterChange(filter.type)}
              disabled={count === 0}
              title={`${filter.label} (${count})`}
              aria-label={`${filter.label} (${count})`}
            >
              {filter.icon && <span className="filter-icon">{filter.icon}</span>}
              <span className="filter-label">{filter.label}</span>
              <span className="filter-count">{count}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
};