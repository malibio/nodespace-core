import { describe, it, expect } from 'vitest';
import {
  isBadgeEnabled,
  versionMismatch,
  BADGE_STORAGE_KEY,
  BADGE_ENABLED_VALUE,
  type BadgeStorage,
} from '$lib/components/dev/version-badge-model';

/** Minimal in-memory Storage stub exposing just the `getItem` the badge reads. */
function stubStorage(value: string | null): BadgeStorage {
  return {
    getItem: (key: string) => (key === BADGE_STORAGE_KEY ? value : null),
  };
}

describe('version-badge-model: isBadgeEnabled', () => {
  it('is true only when the flag equals the enabled value', () => {
    expect(isBadgeEnabled(stubStorage(BADGE_ENABLED_VALUE))).toBe(true);
  });

  it('is false when the flag is unset', () => {
    expect(isBadgeEnabled(stubStorage(null))).toBe(false);
  });

  it('is false for any other flag value (not just "1")', () => {
    expect(isBadgeEnabled(stubStorage('0'))).toBe(false);
    expect(isBadgeEnabled(stubStorage('true'))).toBe(false);
    expect(isBadgeEnabled(stubStorage(''))).toBe(false);
  });

  it('is false when storage is missing or partial', () => {
    expect(isBadgeEnabled(null)).toBe(false);
    expect(isBadgeEnabled(undefined)).toBe(false);
    // A Storage-shaped object where getItem is not callable.
    expect(isBadgeEnabled({ getItem: undefined } as unknown as BadgeStorage)).toBe(false);
  });

  it('is false (never throws) when getItem throws', () => {
    const throwing: BadgeStorage = {
      getItem: () => {
        throw new Error('SecurityError: storage blocked');
      },
    };
    expect(isBadgeEnabled(throwing)).toBe(false);
  });
});

describe('version-badge-model: versionMismatch', () => {
  it('is true when both versions are known and differ', () => {
    expect(versionMismatch('0.2.0', '0.1.0')).toBe(true);
  });

  it('is false when both versions are known and equal', () => {
    expect(versionMismatch('0.2.0', '0.2.0')).toBe(false);
  });

  it('is false when the daemon version is unknown', () => {
    expect(versionMismatch('0.2.0', null)).toBe(false);
    expect(versionMismatch('0.2.0', undefined)).toBe(false);
    expect(versionMismatch('0.2.0', '')).toBe(false);
  });

  it('is false when the frontend version is unknown', () => {
    expect(versionMismatch(null, '0.2.0')).toBe(false);
    expect(versionMismatch(undefined, '0.2.0')).toBe(false);
    expect(versionMismatch('', '0.2.0')).toBe(false);
  });
});
