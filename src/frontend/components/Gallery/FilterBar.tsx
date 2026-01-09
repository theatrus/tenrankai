import React from 'react';

export type FilterType = 'all' | 'picks' | 'rejects' | 'highlighted' | 'commented';

interface FilterBarProps {
  activeFilter: FilterType;
  onFilterChange: (filter: FilterType) => void;
  counts: {
    all: number;
    picks: number;
    rejects: number;
    highlighted: number;
    commented: number;
  };
}

export const FilterBar: React.FC<FilterBarProps> = ({ activeFilter, onFilterChange, counts }) => {
  const filters: { type: FilterType; label: string; icon?: string }[] = [
    { type: 'all', label: 'All' },
    { type: 'picks', label: 'Picks', icon: '✓' },
    { type: 'rejects', label: 'Rejects', icon: '✗' },
    { type: 'highlighted', label: 'Starred', icon: '⭐' },
    { type: 'commented', label: 'Comments', icon: '💬' },
  ];

  return (
    <div className="gallery-filter-bar">
      <div className="filter-buttons">
        {filters.map((filter) => {
          const count = counts[filter.type];
          const isActive = activeFilter === filter.type;
          
          return (
            <button
              key={filter.type}
              className={`filter-btn ${isActive ? 'active' : ''} ${count === 0 ? 'disabled' : ''}`}
              onClick={() => count > 0 && onFilterChange(filter.type)}
              disabled={count === 0}
              title={`${filter.label} (${count})`}
            >
              {filter.icon && <span className="filter-icon">{filter.icon}</span>}
              <span className="filter-label">{filter.label}</span>
              <span className="filter-count">{count}</span>
            </button>
          );
        })}
      </div>
      
      {activeFilter !== 'all' && (
        <button 
          className="clear-filter-btn"
          onClick={() => onFilterChange('all')}
          title="Clear filter"
        >
          Clear filter
        </button>
      )}
    </div>
  );
};