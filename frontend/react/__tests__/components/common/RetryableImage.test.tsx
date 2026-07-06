import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RetryableImage } from '../../../components/common/RetryableImage.tsx';

describe('RetryableImage', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('retries failed image loads with a fragment-only URL change', () => {
    render(<RetryableImage src="/gallery/_image/abc/gallery" alt="queued" />);

    const image = screen.getByRole('img', { name: 'queued' });
    expect(image).toHaveAttribute('src', '/gallery/_image/abc/gallery');

    fireEvent.error(image);
    act(() => {
      vi.advanceTimersByTime(750);
    });

    expect(image).toHaveAttribute('src', '/gallery/_image/abc/gallery#retry-1');
    expect(image).toHaveAttribute('data-retry-attempt', '1');
  });

  it('calls the final error handler after max retries', () => {
    const onError = vi.fn();
    const { container } = render(
      <RetryableImage
        src="/gallery/_image/abc/gallery"
        alt="queued"
        maxRetries={0}
        onError={onError}
      />,
    );

    fireEvent.error(container.querySelector('img')!);

    expect(onError).toHaveBeenCalledTimes(1);
  });
});
