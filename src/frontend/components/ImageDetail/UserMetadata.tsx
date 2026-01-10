import { useState, useRef, useEffect } from 'react';
import { ImageUserMetadata, PickStatus, RolePermissions, ImageArea } from '../../types';
import { AreaSelector } from './AreaSelector.tsx';

interface UserMetadataProps {
  metadata?: ImageUserMetadata;
  imagePath: string;
  galleryName: string;
  isAuthenticated: boolean;
  currentUser?: string;
  permissions: RolePermissions;
  onUpdate: (updatedMetadata: ImageUserMetadata) => void;
  image?: {
    medium_url: string;
    dimensions: [number, number];
  };
}

export function UserMetadata({ 
  metadata, 
  imagePath, 
  galleryName,
  isAuthenticated,
  currentUser,
  permissions,
  onUpdate,
  image
}: UserMetadataProps) {
  const [isEditing, setIsEditing] = useState(false);
  const [newComment, setNewComment] = useState('');
  const [selectedTags, setSelectedTags] = useState<string[]>(metadata?.tags || []);
  const [tagInput, setTagInput] = useState('');
  const [showTagInput, setShowTagInput] = useState(false);
  const [editingCommentId, setEditingCommentId] = useState<string | null>(null);
  const [editingCommentText, setEditingCommentText] = useState('');
  const [editingCommentArea, setEditingCommentArea] = useState<ImageArea | null>(null);
  const [showAreaSelector, setShowAreaSelector] = useState(false);
  const [showEditAreaSelector, setShowEditAreaSelector] = useState(false);
  const [selectedArea, setSelectedArea] = useState<ImageArea | null>(null);
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
          image_area: selectedArea,
        }),
      });

      if (response.ok) {
        const serverResponse = await response.json();
        onUpdate(serverResponse.metadata);
        setNewComment('');
        setIsEditing(false);
        setSelectedArea(null);
        setShowAreaSelector(false);
      }
    } catch (error) {
      console.error('Failed to add comment:', error);
    }
  };

  const handleEditComment = async (commentId: string) => {
    if (!isAuthenticated || !editingCommentText.trim()) return;

    try {
      const response = await fetch(`/api/gallery/${galleryName}/comment/${commentId}/edit/${encodeURIComponent(imagePath)}`, {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          text: editingCommentText.trim(),
          image_area: editingCommentArea,
        }),
      });

      if (response.ok) {
        const serverResponse = await response.json();
        onUpdate(serverResponse.metadata);
        setEditingCommentId(null);
        setEditingCommentText('');
        setEditingCommentArea(null);
        setShowEditAreaSelector(false);
      }
    } catch (error) {
      console.error('Failed to edit comment:', error);
    }
  };

  const handleDeleteComment = async (commentId: string) => {
    if (!isAuthenticated || !confirm('Are you sure you want to delete this comment?')) return;

    try {
      const response = await fetch(`/api/gallery/${galleryName}/comment/${commentId}/delete/${encodeURIComponent(imagePath)}`, {
        method: 'DELETE',
      });

      if (response.ok) {
        const serverResponse = await response.json();
        onUpdate(serverResponse.metadata);
      }
    } catch (error) {
      console.error('Failed to delete comment:', error);
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
            {isAuthenticated && permissions.can_add_tags && (
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
                  <div className="comment-header-right">
                    <span className="comment-date">
                      {formatDate(comment.created_at)}
                      {comment.edited_at && <span className="comment-edited"> (edited)</span>}
                    </span>
                    {currentUser === comment.author && isAuthenticated && (
                      <div className="comment-actions-menu">
                        {editingCommentId !== comment.id && (
                          <>
                            <button
                              className="comment-action-btn"
                              onClick={() => {
                                setEditingCommentId(comment.id);
                                setEditingCommentText(comment.text);
                                setEditingCommentArea(comment.image_area || null);
                              }}
                              title="Edit comment"
                            >
                              Edit
                            </button>
                            <button
                              className="comment-action-btn comment-delete-btn"
                              onClick={() => handleDeleteComment(comment.id)}
                              title="Delete comment"
                            >
                              Delete
                            </button>
                          </>
                        )}
                      </div>
                    )}
                  </div>
                </div>
                {editingCommentId === comment.id ? (
                  <div className="comment-edit-form">
                    <div className="comment-edit-wrapper">
                      <textarea
                        className="comment-textarea"
                        value={editingCommentText}
                        onChange={(e) => setEditingCommentText(e.target.value)}
                        rows={3}
                        autoFocus
                      />
                      
                      {/* Area selector option for editing */}
                      {image && (
                        <div className="comment-area-options">
                          {!showEditAreaSelector && !editingCommentArea && (
                            <button
                              className="area-select-btn"
                              onClick={() => setShowEditAreaSelector(true)}
                              type="button"
                            >
                              📍 Add area selection
                            </button>
                          )}
                          
                          {editingCommentArea && !showEditAreaSelector && (
                            <div className="selected-area-info">
                              <span>✓ Area selected</span>
                              <button
                                className="area-clear-btn"
                                onClick={() => setEditingCommentArea(null)}
                                type="button"
                              >
                                Clear
                              </button>
                              <button
                                className="area-clear-btn"
                                onClick={() => setShowEditAreaSelector(true)}
                                type="button"
                              >
                                Change
                              </button>
                            </div>
                          )}
                        </div>
                      )}
                      
                      {showEditAreaSelector && image && (
                        <div className="area-selector-modal">
                          <AreaSelector
                            imageUrl={image.medium_url}
                            dimensions={image.dimensions}
                            onAreaSelected={(area) => {
                              setEditingCommentArea(area);
                              setShowEditAreaSelector(false);
                            }}
                            existingArea={editingCommentArea}
                          />
                          <button
                            className="area-selector-close"
                            onClick={() => setShowEditAreaSelector(false)}
                          >
                            Close
                          </button>
                        </div>
                      )}
                      
                      <div className="comment-actions">
                        <button
                          className="comment-submit"
                          onClick={() => handleEditComment(comment.id)}
                          disabled={!editingCommentText.trim() || (editingCommentText.trim() === comment.text && editingCommentArea === comment.image_area)}
                        >
                          Save
                        </button>
                        <button
                          className="comment-cancel"
                          onClick={() => {
                            setEditingCommentId(null);
                            setEditingCommentText('');
                            setEditingCommentArea(null);
                            setShowEditAreaSelector(false);
                          }}
                        >
                          Cancel
                        </button>
                      </div>
                    </div>
                  </div>
                ) : (
                  <>
                    <div className="comment-text">{comment.text}</div>
                    {comment.image_area && image && (
                      <div className="comment-area-preview">
                        <div
                          className="area-preview-image"
                          style={{
                            width: '200px',
                            height: `${200 / (image.dimensions[0] / image.dimensions[1])}px`,
                            backgroundImage: `url(${image.medium_url})`,
                            backgroundSize: 'contain',
                            backgroundPosition: 'center',
                            backgroundRepeat: 'no-repeat',
                            position: 'relative',
                            marginTop: '8px'
                          }}
                        >
                          <div
                            className="area-highlight"
                            style={{
                              position: 'absolute',
                              left: `${comment.image_area.x}%`,
                              top: `${comment.image_area.y}%`,
                              width: `${comment.image_area.width}%`,
                              height: `${comment.image_area.height}%`,
                              border: '2px solid rgba(59, 130, 246, 0.8)',
                              backgroundColor: 'rgba(59, 130, 246, 0.1)',
                            }}
                          />
                        </div>
                      </div>
                    )}
                  </>
                )}
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
                
                {/* Area selector option */}
                {image && (
                  <div className="comment-area-options">
                    {!showAreaSelector && !selectedArea && (
                      <button
                        className="area-select-btn"
                        onClick={() => setShowAreaSelector(true)}
                        type="button"
                      >
                        📍 Select area on image
                      </button>
                    )}
                    
                    {selectedArea && !showAreaSelector && (
                      <div className="selected-area-info">
                        <span>✓ Area selected</span>
                        <button
                          className="area-clear-btn"
                          onClick={() => setSelectedArea(null)}
                          type="button"
                        >
                          Clear
                        </button>
                      </div>
                    )}
                  </div>
                )}
                
                {showAreaSelector && image && (
                  <div className="area-selector-modal">
                    <AreaSelector
                      imageUrl={image.medium_url}
                      dimensions={image.dimensions}
                      onAreaSelected={(area) => {
                        setSelectedArea(area);
                        setShowAreaSelector(false);
                      }}
                      existingArea={selectedArea}
                    />
                    <button
                      className="area-selector-close"
                      onClick={() => setShowAreaSelector(false)}
                    >
                      Close
                    </button>
                  </div>
                )}
                
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
                      setSelectedArea(null);
                      setShowAreaSelector(false);
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