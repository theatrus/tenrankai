import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { VideoDisplay } from '../../components/ImageDetail/VideoDisplay';
import { createImageInfo } from '../fixtures/images';

function renderReadyVideo() {
  const image = createImageInfo({ media_kind: 'video', video: {
    state: 'ready', playback_url: '/g/_video/clip.mov',
  } });
  const result = render(<VideoDisplay image={image} />);
  const player = result.container.querySelector('video')!;
  const play = vi.spyOn(player, 'play').mockResolvedValue(undefined);
  const pause = vi.spyOn(player, 'pause').mockImplementation(() => {});
  return { ...result, player, play, pause };
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

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

  it('plays and pauses with Space, prevents scrolling, and ignores held-key repeats', () => {
    const { player, play, pause } = renderReadyVideo();
    expect(fireEvent.keyDown(document.body, { key: ' ' })).toBe(false);
    expect(play).toHaveBeenCalledOnce();
    Object.defineProperty(player, 'paused', { value: false, configurable: true });
    expect(fireEvent.keyDown(document.body, { key: ' ', repeat: true })).toBe(false);
    expect(pause).not.toHaveBeenCalled();
    fireEvent.keyDown(document.body, { key: ' ' });
    expect(pause).toHaveBeenCalledOnce();
  });

  it('leaves focused controls and editable content to handle Space', () => {
    const { play, container } = renderReadyVideo();
    const controls = document.createElement('div');
    controls.innerHTML = '<input><textarea></textarea><select></select><button>Action</button><a href="#">Link</a><div role="button">Action</div><div role="slider"></div><div contenteditable="true"><span>Text</span></div><div contenteditable="plaintext-only">Text</div>';
    container.appendChild(controls);
    for (const target of controls.querySelectorAll('*')) {
      expect(fireEvent.keyDown(target, { key: ' ' })).toBe(true);
    }
    expect(play).not.toHaveBeenCalled();
  });

  it('handles Space on the video host once and prevents the native default toggle', () => {
    const { player, play } = renderReadyVideo();
    expect(fireEvent.keyDown(player, { key: ' ' })).toBe(false);
    expect(play).toHaveBeenCalledOnce();
  });

  it('ignores modified, composing, and already handled Space events and open dialogs', () => {
    const { play, container } = renderReadyVideo();
    for (const options of [{ ctrlKey: true }, { metaKey: true }, { altKey: true }, { shiftKey: true }, { isComposing: true }]) {
      expect(fireEvent.keyDown(document.body, { key: ' ', ...options })).toBe(true);
    }
    const handled = new KeyboardEvent('keydown', { key: ' ', bubbles: true, cancelable: true });
    handled.preventDefault();
    document.body.dispatchEvent(handled);
    const modal = document.createElement('div');
    modal.setAttribute('aria-modal', 'true');
    container.appendChild(modal);
    expect(fireEvent.keyDown(document.body, { key: ' ' })).toBe(true);
    expect(play).not.toHaveBeenCalled();
  });

  it('removes the page shortcut when the video is unmounted', () => {
    const { unmount, play } = renderReadyVideo();
    unmount();
    expect(fireEvent.keyDown(document.body, { key: ' ' })).toBe(true);
    expect(play).not.toHaveBeenCalled();
  });

  it('reports playback failures but ignores an interrupted play request', async () => {
    const { play } = renderReadyVideo();
    play.mockRejectedValueOnce(new DOMException('Paused while starting', 'AbortError'));
    fireEvent.keyDown(document.body, { key: ' ' });
    await waitFor(() => expect(play).toHaveBeenCalledOnce());
    expect(screen.queryByRole('alert')).toBeNull();
    play.mockRejectedValueOnce(new Error('Playback failed'));
    fireEvent.keyDown(document.body, { key: ' ' });
    expect(await screen.findByRole('alert')).toHaveTextContent('could not be played');
  });
});
