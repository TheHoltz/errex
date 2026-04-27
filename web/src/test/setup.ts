// Vitest global setup. Runs once before any test file.
// Add @testing-library matchers (`toBeInTheDocument`, etc.) and reset DOM
// between tests. Because vitest globals are off, @testing-library/svelte's
// auto-cleanup (which checks for a global afterEach) doesn't fire on its own,
// so we wire it up explicitly here.

import '@testing-library/jest-dom/vitest';
import { afterEach } from 'vitest';
import { cleanup } from '@testing-library/svelte';

afterEach(() => cleanup());
