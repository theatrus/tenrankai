import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { VideoDisplay } from '../../components/ImageDetail/VideoDisplay';
import { createImageInfo } from '../fixtures/images';

describe('VideoDisplay', () => {
  it('shows progress and failure without mounting a player', () => {
    const image = createImageInfo({ media_kind: 'video', video: { state: 'processing' } });
    const { container, rerender } = render(<VideoDisplay image={image} />);
    expect(screen.getByRole('status')).toHaveTextContent('Preparing video');
    expect(container.querySelector('video')).toBeNull();
    rerender(<VideoDisplay image={{ ...image, video: { state: 'failed' } }} />);
    expect(screen.getByRole('status')).toHaveTextContent('could not be prepared');
    expect(container.querySelector('video')).toBeNull();
  });

  it('provides native controls and refreshes failed playback', () => {
    const refresh = vi.fn();
    const image = createImageInfo({ media_kind: 'video', title: 'Camera clip', dimensions: [1080, 1920], video: {
      state: 'ready', playback_url: '/g/_video/clip.mov', poster_url: '/g/_image/clip.mov/medium',
    } });
    render(<VideoDisplay image={image} onRefresh={refresh} />);
    const player = screen.getByLabelText('Camera clip');
    expect(player).toHaveAttribute('controls');
    expect(player).toHaveAttribute('playsinline');
    expect(player).toHaveAttribute('preload', 'metadata');
    expect(player).toHaveAttribute('width', '1080');
    expect(player).toHaveAttribute('height', '1920');
    fireEvent.error(player);
    expect(screen.getByRole('alert')).toHaveTextContent('could not be played');
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(refresh).toHaveBeenCalledOnce();
    expect(screen.getByLabelText('Camera clip')).not.toBe(player);
  });
});
