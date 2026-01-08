import { useState, useRef, useEffect } from 'react';
import { ImageUserMetadata, PickStatus } from '../../types';

interface UserMetadataProps {
  metadata?: ImageUserMetadata;
  imagePath: string;
  galleryName: string;
  isAuthenticated: boolean;
  currentUser?: string;
  onUpdate: (updatedMetadata: ImageUserMetadata) => void;
}

export function UserMetadata({ 
  metadata, 
  imagePath, 
  galleryName,
  isAuthenticated,
  onUpdate 
}: UserMetadataProps) {
  const [isEditing, setIsEditing] = useState(false);
  const [newComment, setNewComment] = useState('');
  const [selectedTags, setSelectedTags] = useState<string[]>(metadata?.tags || []);
  const [tagInput, setTagInput] = useState('');
  const [showTagInput, setShowTagInput] = useState(false);
  const commentTextareaRef = useRef<HTMLTextAreaElement>(null);

  // Common photography-related tags
  const suggestedTags = [
    'landscape', 'portrait', 'macro', 'street', 'nature',
    'architecture', 'wildlife', 'travel', 'sunset', 'night',
    'black-and-white', 'urban', 'rural', 'abstract', 'documentary'
  ];

  useEffect(() => {
    setSelectedTags(metadata?.tags || []);
  }, [metadata?.tags]);

  const handlePickStatusChange = async (newStatus: PickStatus | undefined) => {
    if (!isAuthenticated) return;

    // Create new metadata with the updated pick status
    const updatedMetadata: ImageUserMetadata = {
      comments: metadata?.comments || [],
      highlighted: metadata?.highlighted || false,
      tags: metadata?.tags || [],
      pick_status: newStatus,
      last_modified: metadata?.last_modified,
      modified_by: metadata?.modified_by,
    };
    
    // Update local state immediately for better UX
    onUpdate(updatedMetadata);

    try {
      // Only send the pick_status field we're updating
      const updatePayload: any = {};
      if (newStatus !== undefined) {
        updatePayload.pick_status = newStatus;
      } else {
        // Send null to clear the pick status
        updatePayload.pick_status = null;
      }
      
      const response = await fetch(`/api/gallery/${galleryName}/metadata/${encodeURIComponent(imagePath)}`, {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(updatePayload),
      });

      if (response.ok) {
        const serverResponse = await response.json();
        onUpdate(serverResponse.metadata);
      } else {
        // Revert on error
        if (metadata) {
          onUpdate(metadata);
        }
      }
    } catch (error) {
      console.error('Failed to update pick status:', error);
      // Revert on error
      if (metadata) {
        onUpdate(metadata);
      }
    }
  };

  const handleHighlightToggle = async () => {
    if (!isAuthenticated) return;

    // Create new metadata with the toggled highlight
    const updatedMetadata: ImageUserMetadata = {
      comments: metadata?.comments || [],
      highlighted: !metadata?.highlighted,
      tags: metadata?.tags || [],
      pick_status: metadata?.pick_status,
      last_modified: metadata?.last_modified,
      modified_by: metadata?.modified_by,
    };
    
    // Update local state immediately for better UX
    onUpdate(updatedMetadata);

    try {
      // Only send the highlighted field we're updating
      const updatePayload = {
        highlighted: !metadata?.highlighted,
      };
      
      const response = await fetch(`/api/gallery/${galleryName}/metadata/${encodeURIComponent(imagePath)}`, {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(updatePayload),
      });

      if (response.ok) {
        const serverResponse = await response.json();
        onUpdate(serverResponse.metadata);
      } else {
        // Revert on error
        if (metadata) {
          onUpdate(metadata);
        }
      }
    } catch (error) {
      console.error('Failed to update highlight status:', error);
      // Revert on error
      if (metadata) {
        onUpdate(metadata);
      }
    }
  };

  const handleAddComment = async () => {
    if (!isAuthenticated || !newComment.trim()) return;

    try {
      const response = await fetch(`/api/gallery/${galleryName}/comments/${encodeURIComponent(imagePath)}`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          text: newComment.trim(),
        }),
      });

      if (response.ok) {
        const serverResponse = await response.json();
        onUpdate(serverResponse.metadata);
        setNewComment('');
        setIsEditing(false);
      }
    } catch (error) {
      console.error('Failed to add comment:', error);
    }
  };

  const handleTagUpdate = async () => {
    if (!isAuthenticated) return;

    try {
      const response = await fetch(`/api/gallery/${galleryName}/metadata/${encodeURIComponent(imagePath)}`, {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          tags: selectedTags,
        }),
      });

      if (response.ok) {
        const serverResponse = await response.json();
        onUpdate(serverResponse.metadata);
        setShowTagInput(false);
        setTagInput('');
      }
    } catch (error) {
      console.error('Failed to update tags:', error);
    }
  };

  const addTag = (tag: string) => {
    const normalizedTag = tag.toLowerCase().trim();
    if (normalizedTag && !selectedTags.includes(normalizedTag)) {
      setSelectedTags([...selectedTags, normalizedTag]);
    }
    setTagInput('');
  };

  const removeTag = (tag: string) => {
    setSelectedTags(selectedTags.filter(t => t !== tag));
  };

  const formatDate = (dateString: string) => {
    const date = new Date(dateString);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

    if (diffDays === 0) {
      const diffHours = Math.floor(diffMs / (1000 * 60 * 60));
      if (diffHours === 0) {
        const diffMinutes = Math.floor(diffMs / (1000 * 60));
        return diffMinutes <= 1 ? 'just now' : `${diffMinutes} minutes ago`;
      }
      return diffHours === 1 ? '1 hour ago' : `${diffHours} hours ago`;
    } else if (diffDays === 1) {
      return 'yesterday';
    } else if (diffDays < 30) {
      return `${diffDays} days ago`;
    } else {
      return date.toLocaleDateString();
    }
  };

  // Don't render anything for non-authenticated users
  if (!isAuthenticated) {
    return null;
  }

  return (
    <div className="user-metadata">
      {/* Pick Status and Highlight Controls */}
      {isAuthenticated && (
        <div className="metadata-controls">
          <div className="pick-controls">
            <button
              className={`pick-btn ${metadata?.pick_status === 'pick' ? 'active' : ''}`}
              onClick={() => handlePickStatusChange(metadata?.pick_status === 'pick' ? undefined : 'pick')}
              title="Mark as Pick"
            >
              <span className="pick-icon">✓</span> Pick
            </button>
            <button
              className={`pick-btn undecided ${!metadata?.pick_status ? 'active' : ''}`}
              onClick={() => {
                // Only clear if there's currently a pick status set
                if (metadata?.pick_status) {
                  handlePickStatusChange(undefined);
                }
              }}
              title="Clear pick status"
            >
              <span className="pick-icon">?</span> Undecided
            </button>
            <button
              className={`pick-btn no-pick ${metadata?.pick_status === 'no_pick' ? 'active' : ''}`}
              onClick={() => handlePickStatusChange(metadata?.pick_status === 'no_pick' ? undefined : 'no_pick')}
              title="Mark as Reject"
            >
              <span className="pick-icon">✗</span> Reject
            </button>
            <button
              className={`highlight-btn ${metadata?.highlighted ? 'active' : ''}`}
              onClick={handleHighlightToggle}
              title="Toggle Highlight"
            >
              <span className="highlight-icon">★</span>
              {metadata?.highlighted ? 'Highlighted' : 'Highlight'}
            </button>
          </div>
        </div>
      )}

      {/* Tags - Hidden for now */}
      {false && (
        <div className="metadata-tags">
          <h4 className="metadata-header">Tags</h4>
          <div className="tags-container">
            {selectedTags.map(tag => (
              <span key={tag} className="tag">
                {tag}
                {isAuthenticated && (
                  <button
                    className="tag-remove"
                    onClick={() => removeTag(tag)}
                    aria-label={`Remove ${tag} tag`}
                  >
                    ×
                  </button>
                )}
              </span>
            ))}
            {isAuthenticated && (
              <>
                {showTagInput ? (
                  <div className="tag-input-container">
                    <input
                      type="text"
                      className="tag-input"
                      value={tagInput}
                      onChange={(e) => setTagInput(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter') {
                          e.preventDefault();
                          if (tagInput.trim()) {
                            addTag(tagInput);
                          }
                        } else if (e.key === 'Escape') {
                          setShowTagInput(false);
                          setTagInput('');
                        }
                      }}
                      placeholder="Add a tag..."
                      autoFocus
                    />
                    <button
                      className="tag-save-btn"
                      onClick={handleTagUpdate}
                      disabled={selectedTags.length === (metadata?.tags || []).length && 
                               selectedTags.every(t => (metadata?.tags || []).includes(t))}
                    >
                      Save
                    </button>
                    <button
                      className="tag-cancel-btn"
                      onClick={() => {
                        setSelectedTags(metadata?.tags || []);
                        setShowTagInput(false);
                        setTagInput('');
                      }}
                    >
                      Cancel
                    </button>
                  </div>
                ) : (
                  <button
                    className="add-tag-btn"
                    onClick={() => setShowTagInput(true)}
                  >
                    + Add Tag
                  </button>
                )}
              </>
            )}
          </div>
          {isAuthenticated && showTagInput && (
            <div className="suggested-tags">
              {suggestedTags
                .filter(tag => !selectedTags.includes(tag) && tag.includes(tagInput.toLowerCase()))
                .slice(0, 5)
                .map(tag => (
                  <button
                    key={tag}
                    className="suggested-tag"
                    onClick={() => addTag(tag)}
                  >
                    {tag}
                  </button>
                ))}
            </div>
          )}
        </div>
      )}

      {/* Comments Section */}
      <div className="metadata-comments">
        <h4 className="metadata-header">
          Comments
          {metadata?.comments && metadata.comments.length > 0 && (
            <span className="comment-count">({metadata.comments.length})</span>
          )}
        </h4>
        
        {/* Comments List */}
        {metadata?.comments && metadata.comments.length > 0 && (
          <div className="comments-list">
            {metadata.comments.map(comment => (
              <div key={comment.id} className="comment">
                <div className="comment-header">
                  <span className="comment-author">{comment.author}</span>
                  <span className="comment-date">{formatDate(comment.created_at)}</span>
                </div>
                <div className="comment-text">{comment.text}</div>
              </div>
            ))}
          </div>
        )}

        {/* Add Comment Form */}
        {isAuthenticated && (
          <div className="add-comment">
            {isEditing ? (
              <div className="comment-form">
                <textarea
                  ref={commentTextareaRef}
                  className="comment-textarea"
                  value={newComment}
                  onChange={(e) => setNewComment(e.target.value)}
                  placeholder="Write a comment..."
                  rows={3}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' && e.ctrlKey) {
                      e.preventDefault();
                      handleAddComment();
                    }
                  }}
                />
                <div className="comment-actions">
                  <button
                    className="comment-submit"
                    onClick={handleAddComment}
                    disabled={!newComment.trim()}
                  >
                    Add Comment
                  </button>
                  <button
                    className="comment-cancel"
                    onClick={() => {
                      setNewComment('');
                      setIsEditing(false);
                    }}
                  >
                    Cancel
                  </button>
                  <span className="comment-hint">Ctrl+Enter to submit</span>
                </div>
              </div>
            ) : (
              <button
                className="add-comment-btn"
                onClick={() => {
                  setIsEditing(true);
                  setTimeout(() => commentTextareaRef.current?.focus(), 0);
                }}
              >
                Add a comment...
              </button>
            )}
          </div>
        )}

      </div>
    </div>
  );
}