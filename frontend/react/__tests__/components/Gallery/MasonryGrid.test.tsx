import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MasonryGrid, GalleryImage } from '@components/Gallery/MasonryGrid';

function createGalleryImage(overrides?: Partial<GalleryImage>): GalleryImage {
  return {
    path: 'image-1.jpg',
    name: 'image-1.jpg',
    thumbnail_url: '/gallery/main/thumb/image-1.jpg?size=thumbnail',
    gallery_url: '/gallery/main/thumb/image-1.jpg?size=gallery',
    dimensions: [1200, 800],
    is_new: false,
    is_directory: false,
    ...overrides,
  };
}

describe('MasonryGrid', () => {
  it('renders gallery images', () => {
    const images = [
      createGalleryImage({ path: 'img1.jpg', name: 'img1.jpg' }),
      createGalleryImage({ path: 'img2.jpg', name: 'img2.jpg' }),
    ];

    render(<MasonryGrid images={images} galleryUrl="/gallery/main" />);

    const links = screen.getAllByRole('link');
    expect(links.length).toBeGreaterThanOrEqual(2);
    expect(links[0]).toHaveAttribute('href', '/gallery/main/detail/img1.jpg');
  });

  it('renders image containers with aria-label', () => {
    const images = [createGalleryImage({ path: 'sunset.jpg', name: 'sunset.jpg' })];

    render(<MasonryGrid images={images} galleryUrl="/gallery/main" />);

    expect(screen.getByRole('img', { name: 'sunset.jpg' })).toBeInTheDocument();
  });

  it('applies is-new class for new images', () => {
    const images = [createGalleryImage({ path: 'new.jpg', name: 'new.jpg', is_new: true })];

    const { container } = render(<MasonryGrid images={images} galleryUrl="/gallery/main" />);

    const imageItem = container.querySelector('.image-item.is-new');
    expect(imageItem).not.toBeNull();
  });

  it('applies is-hidden class for hidden images', () => {
    const images = [createGalleryImage({ path: 'hidden.jpg', name: 'hidden.jpg' })];

    const { container } = render(
      <MasonryGrid images={images} galleryUrl="/gallery/main" hiddenImages={['hidden.jpg']} />,
    );

    const imageItem = container.querySelector('.image-item.is-hidden');
    expect(imageItem).not.toBeNull();
  });

  it('renders selection checkbox in manage mode', () => {
    const images = [createGalleryImage({ path: 'img.jpg', name: 'img.jpg' })];

    const { container } = render(
      <MasonryGrid images={images} galleryUrl="/gallery/main" isManageMode={true} />,
    );

    const checkbox = container.querySelector('.selection-checkbox');
    expect(checkbox).not.toBeNull();
  });

  it('calls onToggleSelect when clicking in manage mode', () => {
    const onToggleSelect = vi.fn();
    const images = [createGalleryImage({ path: 'img.jpg', name: 'img.jpg' })];

    const { container } = render(
      <MasonryGrid
        images={images}
        galleryUrl="/gallery/main"
        isManageMode={true}
        onToggleSelect={onToggleSelect}
      />,
    );

    const imageItem = container.querySelector('.image-item')!;
    fireEvent.click(imageItem);

    expect(onToggleSelect).toHaveBeenCalledWith('img.jpg');
  });

  it('shows selected state for selected images', () => {
    const images = [createGalleryImage({ path: 'img.jpg', name: 'img.jpg' })];

    const { container } = render(
      <MasonryGrid
        images={images}
        galleryUrl="/gallery/main"
        isManageMode={true}
        selectedImages={new Set(['img.jpg'])}
      />,
    );

    const imageItem = container.querySelector('.image-item.selected');
    expect(imageItem).not.toBeNull();

    const checkbox = container.querySelector('.selection-checkbox');
    expect(checkbox?.textContent).toBe('✓');
  });

  it('renders badges when permissions allow and metadata exists', () => {
    const images = [
      createGalleryImage({
        path: 'img.jpg',
        name: 'img.jpg',
        user_metadata: {
          comments: [{ id: '1', author: 'user', text: 'Nice', created_at: '2024-01-01' }],
          highlighted: true,
          pick_status: 'pick',
          tags: ['nature'],
        },
      }),
    ];

    const { container } = render(
      <MasonryGrid
        images={images}
        galleryUrl="/gallery/main"
        permissions={{ can_read_metadata: true }}
      />,
    );

    const badges = container.querySelector('.image-badges');
    expect(badges).not.toBeNull();
    expect(container.querySelector('.badge-comments')).not.toBeNull();
    expect(container.querySelector('.badge-highlighted')).not.toBeNull();
    expect(container.querySelector('.badge-pick')).not.toBeNull();
    expect(container.querySelector('.badge-tags')).not.toBeNull();
  });

  it('does not render badges when permissions disallow metadata', () => {
    const images = [
      createGalleryImage({
        path: 'img.jpg',
        name: 'img.jpg',
        user_metadata: {
          comments: [{ id: '1', author: 'user', text: 'Nice', created_at: '2024-01-01' }],
          highlighted: true,
          tags: [],
        },
      }),
    ];

    const { container } = render(
      <MasonryGrid
        images={images}
        galleryUrl="/gallery/main"
        permissions={{ can_read_metadata: false }}
      />,
    );

    const badges = container.querySelector('.image-badges');
    expect(badges).toBeNull();
  });

  it('renders undecided pick status badge', () => {
    const images = [
      createGalleryImage({
        path: 'img.jpg',
        name: 'img.jpg',
        user_metadata: {
          comments: [],
          highlighted: false,
          pick_status: 'undecided',
          tags: [],
        },
      }),
    ];

    const { container } = render(
      <MasonryGrid
        images={images}
        galleryUrl="/gallery/main"
        permissions={{ can_read_metadata: true }}
      />,
    );

    const undecidedBadge = container.querySelector('.badge-undecided');
    expect(undecidedBadge).not.toBeNull();
    expect(undecidedBadge?.textContent?.trim()).toBe('?');
  });

  it('renders no_pick (rejected) badge', () => {
    const images = [
      createGalleryImage({
        path: 'img.jpg',
        name: 'img.jpg',
        user_metadata: {
          comments: [],
          highlighted: false,
          pick_status: 'no_pick',
          tags: [],
        },
      }),
    ];

    const { container } = render(
      <MasonryGrid
        images={images}
        galleryUrl="/gallery/main"
        permissions={{ can_read_metadata: true }}
      />,
    );

    const rejectBadge = container.querySelector('.badge-reject');
    expect(rejectBadge).not.toBeNull();
  });

  it('renders with empty images array', () => {
    const { container } = render(<MasonryGrid images={[]} galleryUrl="/gallery/main" />);

    const grid = container.querySelector('.image-grid');
    expect(grid).not.toBeNull();
  });
});
