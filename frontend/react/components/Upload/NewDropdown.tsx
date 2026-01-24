import React, { useState, useRef, useEffect } from 'react';

interface NewDropdownProps {
  onUpload: () => void;
  onNewFolder: () => void;
}

export const NewDropdown: React.FC<NewDropdownProps> = ({ onUpload, onNewFolder }) => {
  const [isOpen, setIsOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    };

    if (isOpen) {
      document.addEventListener('mousedown', handleClickOutside);
    }

    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
    };
  }, [isOpen]);

  const handleUpload = () => {
    setIsOpen(false);
    onUpload();
  };

  const handleNewFolder = () => {
    setIsOpen(false);
    onNewFolder();
  };

  return (
    <div className="dropdown" ref={dropdownRef} style={{ position: 'relative', display: 'inline-block' }}>
      <button
        className="btn btn-secondary"
        onClick={() => setIsOpen(!isOpen)}
        style={{ display: 'flex', alignItems: 'center', gap: '0.25rem' }}
      >
        + New
        <span style={{ fontSize: '0.7em', marginLeft: '0.25rem' }}>
          {isOpen ? '\u25B2' : '\u25BC'}
        </span>
      </button>
      {isOpen && (
        <div
          className="dropdown-menu"
          style={{
            position: 'absolute',
            top: '100%',
            left: 0,
            marginTop: '4px',
            minWidth: '160px',
            backgroundColor: 'var(--bg-color, #fff)',
            border: '1px solid var(--border-color, #ccc)',
            borderRadius: '4px',
            boxShadow: '0 2px 8px rgba(0,0,0,0.15)',
            zIndex: 1000,
          }}
        >
          <button
            className="dropdown-item"
            onClick={handleUpload}
            style={{
              display: 'block',
              width: '100%',
              padding: '0.5rem 1rem',
              border: 'none',
              background: 'none',
              textAlign: 'left',
              cursor: 'pointer',
              fontSize: '0.9rem',
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.backgroundColor = 'var(--hover-bg, #f0f0f0)';
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.backgroundColor = 'transparent';
            }}
          >
            Upload Images
          </button>
          <button
            className="dropdown-item"
            onClick={handleNewFolder}
            style={{
              display: 'block',
              width: '100%',
              padding: '0.5rem 1rem',
              border: 'none',
              background: 'none',
              textAlign: 'left',
              cursor: 'pointer',
              fontSize: '0.9rem',
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.backgroundColor = 'var(--hover-bg, #f0f0f0)';
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.backgroundColor = 'transparent';
            }}
          >
            New Folder
          </button>
        </div>
      )}
    </div>
  );
};
