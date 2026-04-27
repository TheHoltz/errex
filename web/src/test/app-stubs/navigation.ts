// Test stub for `$app/navigation`. Tests that exercise nav can spy on
// these functions; default implementations are no-ops so component code
// runs without throwing.
import { vi } from 'vitest';

export const goto = vi.fn(async () => {});
export const invalidate = vi.fn(async () => {});
export const invalidateAll = vi.fn(async () => {});
export const beforeNavigate = vi.fn();
export const afterNavigate = vi.fn();
