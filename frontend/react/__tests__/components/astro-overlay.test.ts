import { describe, it, expect } from 'vitest';
import {
  catalogGroup,
  partitionObjects,
  DEFAULT_LABEL_DENSITY,
  type AstroSolution,
} from '../../components/ImageDetail/AstroOverlay.tsx';

type Obj = NonNullable<AstroSolution['objects']>[number];

function obj(partial: Partial<Obj> & { name: string }): Obj {
  return {
    common_name: '',
    kind: 'galaxy',
    x: 500,
    y: 500,
    semi_major_px: 20,
    semi_minor_px: 20,
    angle_deg: 0,
    ...partial,
  };
}

function solution(objects: Obj[]): AstroSolution {
  return { solved: true, width: 2000, height: 2000, objects };
}

describe('catalogGroup', () => {
  it('routes the new LBN and Cederblad catalogs to their own groups', () => {
    expect(catalogGroup(obj({ name: 'LBN 468', kind: 'nebula' }))).toBe('lbn');
    expect(catalogGroup(obj({ name: 'Ced 214', kind: 'nebula' }))).toBe('cederblad');
    expect(catalogGroup(obj({ name: 'Ced 19c', kind: 'nebula' }))).toBe('cederblad');
  });

  it('keeps existing catalogs unchanged', () => {
    expect(catalogGroup(obj({ name: 'NGC 7331' }))).toBe('ngc-ic-messier');
    expect(catalogGroup(obj({ name: 'LDN 935', kind: 'dark-nebula' }))).toBe('dark-nebulae');
    expect(catalogGroup(obj({ name: 'Sh2-101', kind: 'hii-region' }))).toBe('sharpless-vdb');
    expect(catalogGroup(obj({ name: 'PGC 2297311' }))).toBe('pgc');
  });
});

describe('partitionObjects density budget', () => {
  const opts = { hiddenGroups: [] as string[], allTransients: false };

  it('shows every object at full density', () => {
    const objs = Array.from({ length: 10 }, (_, i) =>
      obj({ name: `NGC ${i}`, prominence: i / 10 }),
    );
    const { rendered } = partitionObjects(solution(objs), { ...opts, density: 1 });
    expect(rendered.length).toBe(10);
  });

  it('keeps only the most prominent named features at low density', () => {
    const objs = Array.from({ length: 12 }, (_, i) =>
      obj({ name: `NGC ${i}`, prominence: i / 12 }),
    );
    const { rendered } = partitionObjects(solution(objs), { ...opts, density: 0 });
    // Floor of 4 at the lowest density, taken from the top of the ranking.
    expect(rendered.length).toBe(4);
    const proms = rendered.map((o) => o.prominence as number).sort((a, b) => b - a);
    expect(proms[proms.length - 1]).toBeGreaterThan(0.5);
  });

  it('ranks by prominence, not catalog order', () => {
    const objs = [
      obj({ name: 'faint', prominence: 0.05 }),
      obj({ name: 'bright', prominence: 0.9 }),
      obj({ name: 'mid', prominence: 0.4 }),
    ];
    const { rendered } = partitionObjects(solution(objs), { ...opts, density: 0 });
    // floor is min(3,4)=3 so all shown, but bright must sort ahead of faint
    expect(rendered.findIndex((o) => o.name === 'bright')).toBeLessThan(
      rendered.findIndex((o) => o.name === 'faint'),
    );
  });

  it('always keeps transients and minor bodies regardless of density', () => {
    const objs = [
      ...Array.from({ length: 20 }, (_, i) => obj({ name: `NGC ${i}`, prominence: i / 20 })),
      obj({ name: 'SN 2026abc', kind: 'transient', near_capture: true, prominence: null }),
      obj({ name: 'C/2025 A6', kind: 'comet', prominence: null }),
    ];
    const { rendered } = partitionObjects(solution(objs), { ...opts, density: 0 });
    expect(rendered.some((o) => o.kind === 'transient')).toBe(true);
    expect(rendered.some((o) => o.kind === 'comet')).toBe(true);
  });

  it('has a sane default density that thins a crowded field', () => {
    const objs = Array.from({ length: 30 }, (_, i) =>
      obj({ name: `NGC ${i}`, prominence: (i % 10) / 10 }),
    );
    const { rendered, total } = partitionObjects(solution(objs), {
      ...opts,
      density: DEFAULT_LABEL_DENSITY,
    });
    expect(total).toBe(30);
    expect(rendered.length).toBeGreaterThan(4);
    expect(rendered.length).toBeLessThan(30);
  });
});
