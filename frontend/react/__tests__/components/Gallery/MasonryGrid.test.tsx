import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, within } from '@testing-library/react';
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

function setViewportWidth(width: number) {
  Object.defineProperty(window, 'innerWidth', {
    writable: true,
    configurable: true,
    value: width,
  });
  window.dispatchEvent(new Event('resize'));
}

describe('MasonryGrid', () => {
  let originalInnerWidth: number;

  beforeEach(() => {
    originalInnerWidth = window.innerWidth;
    vi.useFakeTimers();
  });

  afterEach(() => {
    Object.defineProperty(window, 'innerWidth', {
      writable: true,
      configurable: true,
      value: originalInnerWidth,
    });
    vi.useRealTimers();
  });

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

  describe('variable column count', () => {
    it('renders 1 column at mobile viewport (<=480px)', () => {
      setViewportWidth(400);
      vi.advanceTimersByTime(200);

      const images = [
        createGalleryImage({ path: 'img1.jpg', name: 'img1.jpg' }),
        createGalleryImage({ path: 'img2.jpg', name: 'img2.jpg' }),
      ];

      const { container } = render(<MasonryGrid images={images} galleryUrl="/gallery/main" />);

      const visibleColumns = container.querySelectorAll('.masonry-column[style*="display: flex"]');
      expect(visibleColumns).toHaveLength(1);
    });

    it('renders 2 columns at small tablet viewport (481-768px)', () => {
      setViewportWidth(600);
      vi.advanceTimersByTime(200);

      const images = [
        createGalleryImage({ path: 'img1.jpg', name: 'img1.jpg' }),
        createGalleryImage({ path: 'img2.jpg', name: 'img2.jpg' }),
      ];

      const { container } = render(<MasonryGrid images={images} galleryUrl="/gallery/main" />);

      const visibleColumns = container.querySelectorAll('.masonry-column[style*="display: flex"]');
      expect(visibleColumns).toHaveLength(2);
    });

    it('renders 3 columns at tablet viewport (769-1024px)', () => {
      setViewportWidth(900);
      vi.advanceTimersByTime(200);

      const images = Array.from({ length: 6 }, (_, i) =>
        createGalleryImage({ path: `img${i}.jpg`, name: `img${i}.jpg` }),
      );

      const { container } = render(<MasonryGrid images={images} galleryUrl="/gallery/main" />);

      const visibleColumns = container.querySelectorAll('.masonry-column[style*="display: flex"]');
      expect(visibleColumns).toHaveLength(3);
    });

    it('renders 4 columns at desktop viewport (1025-1400px)', () => {
      setViewportWidth(1200);
      vi.advanceTimersByTime(200);

      const images = Array.from({ length: 8 }, (_, i) =>
        createGalleryImage({ path: `img${i}.jpg`, name: `img${i}.jpg` }),
      );

      const { container } = render(<MasonryGrid images={images} galleryUrl="/gallery/main" />);

      const visibleColumns = container.querySelectorAll('.masonry-column[style*="display: flex"]');
      expect(visibleColumns).toHaveLength(4);
    });

    it('renders 5 columns at wide viewport (>1400px)', () => {
      setViewportWidth(1600);
      vi.advanceTimersByTime(200);

      const images = Array.from({ length: 10 }, (_, i) =>
        createGalleryImage({ path: `img${i}.jpg`, name: `img${i}.jpg` }),
      );

      const { container } = render(<MasonryGrid images={images} galleryUrl="/gallery/main" />);

      const visibleColumns = container.querySelectorAll('.masonry-column[style*="display: flex"]');
      expect(visibleColumns).toHaveLength(5);
    });

    it('uses explicit columnCount override when provided', () => {
      setViewportWidth(1600);
      vi.advanceTimersByTime(200);

      const images = Array.from({ length: 6 }, (_, i) =>
        createGalleryImage({ path: `img${i}.jpg`, name: `img${i}.jpg` }),
      );

      const { container } = render(
        <MasonryGrid images={images} galleryUrl="/gallery/main" columnCount={3} />,
      );

      const visibleColumns = container.querySelectorAll('.masonry-column[style*="display: flex"]');
      expect(visibleColumns).toHaveLength(3);
    });

    it('enforces minimum of 1 column for explicit columnCount', () => {
      const images = [createGalleryImage({ path: 'img.jpg', name: 'img.jpg' })];

      const { container } = render(
        <MasonryGrid images={images} galleryUrl="/gallery/main" columnCount={0} />,
      );

      const visibleColumns = container.querySelectorAll('.masonry-column[style*="display: flex"]');
      expect(visibleColumns).toHaveLength(1);
    });
  });

  describe('square grid mode', () => {
    it('renders with square-grid class when gridMode is square', () => {
      const images = [
        createGalleryImage({ path: 'img1.jpg', name: 'img1.jpg' }),
        createGalleryImage({ path: 'img2.jpg', name: 'img2.jpg' }),
      ];

      const { container } = render(
        <MasonryGrid images={images} galleryUrl="/gallery/main" gridMode="square" />,
      );

      const grid = container.querySelector('.image-grid.square-grid');
      expect(grid).not.toBeNull();
    });

    it('does not render masonry columns in square mode', () => {
      const images = [
        createGalleryImage({ path: 'img1.jpg', name: 'img1.jpg' }),
        createGalleryImage({ path: 'img2.jpg', name: 'img2.jpg' }),
      ];

      const { container } = render(
        <MasonryGrid images={images} galleryUrl="/gallery/main" gridMode="square" />,
      );

      const masonryColumns = container.querySelectorAll('.masonry-column');
      expect(masonryColumns).toHaveLength(0);
    });

    it('renders all images directly in the grid container for square mode', () => {
      const images = [
        createGalleryImage({ path: 'img1.jpg', name: 'img1.jpg' }),
        createGalleryImage({ path: 'img2.jpg', name: 'img2.jpg' }),
        createGalleryImage({ path: 'img3.jpg', name: 'img3.jpg' }),
      ];

      const { container } = render(
        <MasonryGrid images={images} galleryUrl="/gallery/main" gridMode="square" />,
      );

      const grid = container.querySelector('.square-grid');
      const items = grid?.querySelectorAll('.image-item');
      expect(items).toHaveLength(3);
    });

    it('sets --grid-columns CSS variable on the grid', () => {
      const images = [createGalleryImage({ path: 'img.jpg', name: 'img.jpg' })];

      const { container } = render(
        <MasonryGrid images={images} galleryUrl="/gallery/main" gridMode="square" />,
      );

      const grid = container.querySelector('.square-grid') as HTMLElement;
      expect(grid).not.toBeNull();
      expect(grid.style.getPropertyValue('--grid-columns')).toBeTruthy();
    });

    it('does not set explicit width/height on items in square mode', () => {
      const images = [
        createGalleryImage({ path: 'img.jpg', name: 'img.jpg', dimensions: [1200, 800] }),
      ];

      const { container } = render(
        <MasonryGrid images={images} galleryUrl="/gallery/main" gridMode="square" />,
      );

      const item = container.querySelector('.image-item') as HTMLElement;
      expect(item.style.width).toBe('');
      expect(item.style.height).toBe('');
    });

    it('renders badges in square mode', () => {
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
          gridMode="square"
          permissions={{ can_read_metadata: true }}
        />,
      );

      expect(container.querySelector('.image-badges')).not.toBeNull();
      expect(container.querySelector('.badge-comments')).not.toBeNull();
      expect(container.querySelector('.badge-highlighted')).not.toBeNull();
      expect(container.querySelector('.badge-pick')).not.toBeNull();
      expect(container.querySelector('.badge-tags')).not.toBeNull();
    });

    it('renders selection checkbox in square mode manage mode', () => {
      const images = [createGalleryImage({ path: 'img.jpg', name: 'img.jpg' })];

      const { container } = render(
        <MasonryGrid
          images={images}
          galleryUrl="/gallery/main"
          gridMode="square"
          isManageMode={true}
        />,
      );

      const checkbox = container.querySelector('.selection-checkbox');
      expect(checkbox).not.toBeNull();
    });

    it('calls onToggleSelect when clicking in square mode manage mode', () => {
      const onToggleSelect = vi.fn();
      const images = [createGalleryImage({ path: 'img.jpg', name: 'img.jpg' })];

      const { container } = render(
        <MasonryGrid
          images={images}
          galleryUrl="/gallery/main"
          gridMode="square"
          isManageMode={true}
          onToggleSelect={onToggleSelect}
        />,
      );

      const imageItem = container.querySelector('.image-item')!;
      fireEvent.click(imageItem);

      expect(onToggleSelect).toHaveBeenCalledWith('img.jpg');
    });

    it('shows selected state in square mode', () => {
      const images = [createGalleryImage({ path: 'img.jpg', name: 'img.jpg' })];

      const { container } = render(
        <MasonryGrid
          images={images}
          galleryUrl="/gallery/main"
          gridMode="square"
          isManageMode={true}
          selectedImages={new Set(['img.jpg'])}
        />,
      );

      const imageItem = container.querySelector('.image-item.selected');
      expect(imageItem).not.toBeNull();
    });

    it('renders image containers with correct role in square mode', () => {
      const images = [
        createGalleryImage({
          path: 'img.jpg',
          name: 'img.jpg',
          thumbnail_url: '/gallery/main/thumb/img.jpg?size=thumbnail',
          gallery_url: '/gallery/main/thumb/img.jpg?size=gallery',
        }),
      ];

      const { container } = render(
        <MasonryGrid images={images} galleryUrl="/gallery/main" gridMode="square" />,
      );

      const imageContainer = container.querySelector('.gallery-image-container');
      expect(imageContainer).not.toBeNull();
      expect(within(container).getByRole('img', { name: 'img.jpg' })).toBe(imageContainer);
      expect(imageContainer).toHaveAttribute('alt', 'img.jpg');
    });

    it('builds retina srcset for path-based image URLs', () => {
      const images = [
        createGalleryImage({
          path: 'img.jpg',
          name: 'img.jpg',
          thumbnail_url: '/gallery/_image/abc123/thumbnail',
          gallery_url: '/gallery/_image/abc123/gallery',
        }),
      ];

      const { container } = render(
        <MasonryGrid images={images} galleryUrl="/gallery/main" gridMode="square" />,
      );

      const imageContainer = container.querySelector('.gallery-image-container');
      expect(imageContainer).toHaveAttribute(
        'srcset',
        '/gallery/_image/abc123/thumbnail 1x, /gallery/_image/abc123/thumbnail@2x 2x',
      );
    });
  });

  describe('maxColumns cap', () => {
    it('caps responsive columns in masonry mode', () => {
      setViewportWidth(1600);
      vi.advanceTimersByTime(200);

      const images = Array.from({ length: 10 }, (_, i) =>
        createGalleryImage({ path: `img${i}.jpg`, name: `img${i}.jpg` }),
      );

      const { container } = render(
        <MasonryGrid images={images} galleryUrl="/gallery/main" maxColumns={3} />,
      );

      const visibleColumns = container.querySelectorAll('.masonry-column[style*="display: flex"]');
      expect(visibleColumns).toHaveLength(3);
    });

    it('caps responsive columns in square mode', () => {
      setViewportWidth(1600);
      vi.advanceTimersByTime(200);

      const images = Array.from({ length: 6 }, (_, i) =>
        createGalleryImage({ path: `img${i}.jpg`, name: `img${i}.jpg` }),
      );

      const { container } = render(
        <MasonryGrid images={images} galleryUrl="/gallery/main" gridMode="square" maxColumns={2} />,
      );

      const grid = container.querySelector('.square-grid') as HTMLElement;
      expect(grid.style.getPropertyValue('--grid-columns')).toBe('2');
    });

    it('does not increase columns beyond responsive breakpoint', () => {
      setViewportWidth(400);
      vi.advanceTimersByTime(200);

      const images = [
        createGalleryImage({ path: 'img1.jpg', name: 'img1.jpg' }),
        createGalleryImage({ path: 'img2.jpg', name: 'img2.jpg' }),
      ];

      const { container } = render(
        <MasonryGrid images={images} galleryUrl="/gallery/main" maxColumns={5} />,
      );

      const visibleColumns = container.querySelectorAll('.masonry-column[style*="display: flex"]');
      expect(visibleColumns).toHaveLength(1);
    });

    it('does not cap explicit columnCount (maxColumns only applies to responsive)', () => {
      setViewportWidth(1600);
      vi.advanceTimersByTime(200);

      const images = Array.from({ length: 8 }, (_, i) =>
        createGalleryImage({ path: `img${i}.jpg`, name: `img${i}.jpg` }),
      );

      const { container } = render(
        <MasonryGrid images={images} galleryUrl="/gallery/main" columnCount={4} maxColumns={2} />,
      );

      const visibleColumns = container.querySelectorAll('.masonry-column[style*="display: flex"]');
      expect(visibleColumns).toHaveLength(4);
    });

    it('has no effect when undefined', () => {
      setViewportWidth(1600);
      vi.advanceTimersByTime(200);

      const images = Array.from({ length: 10 }, (_, i) =>
        createGalleryImage({ path: `img${i}.jpg`, name: `img${i}.jpg` }),
      );

      const { container } = render(
        <MasonryGrid images={images} galleryUrl="/gallery/main" />,
      );

      const visibleColumns = container.querySelectorAll('.masonry-column[style*="display: flex"]');
      expect(visibleColumns).toHaveLength(5);
    });
  });

  describe('default behavior', () => {
    it('defaults to masonry mode when no gridMode prop is provided', () => {
      const images = [
        createGalleryImage({ path: 'img1.jpg', name: 'img1.jpg' }),
        createGalleryImage({ path: 'img2.jpg', name: 'img2.jpg' }),
      ];

      const { container } = render(<MasonryGrid images={images} galleryUrl="/gallery/main" />);

      expect(container.querySelector('.square-grid')).toBeNull();
      expect(container.querySelector('.masonry-column')).not.toBeNull();
    });

    it('renders masonry columns with data-column attributes', () => {
      const images = [
        createGalleryImage({ path: 'img1.jpg', name: 'img1.jpg' }),
        createGalleryImage({ path: 'img2.jpg', name: 'img2.jpg' }),
      ];

      const { container } = render(<MasonryGrid images={images} galleryUrl="/gallery/main" />);

      const columns = container.querySelectorAll('.masonry-column');
      expect(columns.length).toBeGreaterThanOrEqual(1);
      expect(columns[0].getAttribute('data-column')).toBe('0');
    });
  });
});
