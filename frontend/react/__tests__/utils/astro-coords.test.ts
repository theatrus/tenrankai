import { describe, it, expect } from 'vitest';
import { parseRa, parseDec, formatDegrees } from '../../utils/astro-coords.ts';

describe('parseRa', () => {
  it('parses sexagesimal hours', () => {
    expect(parseRa('00h 42m 44s')).toBeCloseTo(10.6833, 3);
    expect(parseRa('5h34m32s')).toBeCloseTo(83.6333, 3);
    expect(parseRa('12h')).toBeCloseTo(180, 6);
  });

  it('parses colon-separated hours', () => {
    expect(parseRa('05:34:32')).toBeCloseTo(83.6333, 3);
    expect(parseRa('0:42')).toBeCloseTo(10.5, 4);
  });

  it('parses decimal and sexagesimal degrees', () => {
    expect(parseRa('83.82')).toBeCloseTo(83.82, 4);
    expect(parseRa('83° 49′ 12″')).toBeCloseTo(83.82, 3);
  });

  it('wraps into [0, 360)', () => {
    expect(parseRa('24h')).toBeCloseTo(0, 6);
    expect(parseRa('360')).toBeCloseTo(0, 6);
  });

  it('rejects garbage', () => {
    expect(parseRa('')).toBeNull();
    expect(parseRa('north')).toBeNull();
    expect(parseRa('-5h')).toBeNull();
  });
});

describe('parseDec', () => {
  it('parses signed sexagesimal degrees', () => {
    expect(parseDec("+41° 16' 09\"")).toBeCloseTo(41.2692, 3);
    expect(parseDec('-05d 23m 28s')).toBeCloseTo(-5.3911, 3);
    expect(parseDec('−22° 00′')).toBeCloseTo(-22.0, 4);
  });

  it('parses colon-separated and decimal degrees', () => {
    expect(parseDec('-5:23:28')).toBeCloseTo(-5.3911, 3);
    expect(parseDec('41.269')).toBeCloseTo(41.269, 4);
  });

  it('rejects out-of-range and garbage values', () => {
    expect(parseDec('95')).toBeNull();
    expect(parseDec('-91')).toBeNull();
    expect(parseDec('')).toBeNull();
    expect(parseDec('zenith')).toBeNull();
  });
});

describe('formatDegrees', () => {
  it('trims trailing zeros', () => {
    expect(formatDegrees(83.82)).toBe('83.82');
    expect(formatDegrees(180)).toBe('180');
    expect(formatDegrees(-5.3911)).toBe('-5.3911');
  });
});
