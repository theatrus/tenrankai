import { describe, expect, it } from 'vitest';
import { formatHours } from '../../utils/format-hours';

describe('formatHours', () => {
  it('shows hours and minutes', () => {
    expect(formatHours(5.17)).toBe('5h 10m');
    expect(formatHours(11.5)).toBe('11h 30m');
  });

  it('drops zero minutes', () => {
    expect(formatHours(48)).toBe('48h');
    expect(formatHours(1.999)).toBe('2h');
  });

  it('shows minutes alone under an hour', () => {
    expect(formatHours(0.13)).toBe('8m');
    expect(formatHours(0.75)).toBe('45m');
  });

  it('never shows 0h for very short totals', () => {
    expect(formatHours(0.005)).toBe('<1m');
  });
});
