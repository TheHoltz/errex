// Verifies that the post-sign-in bootstrap fans out the side effects the
// signed-in shell relies on (project list, WS connection, initial-load
// handoff). The bug it pins: if these only run inside the layout's
// onMount, a user who lands on /setup or /login and then signs in lives
// in a half-bootstrapped state until they refresh the tab.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const { connectMock } = vi.hoisted(() => ({ connectMock: vi.fn() }));
vi.mock('./ws', () => ({ connect: connectMock, disconnect: vi.fn() }));

import { api } from './api';
import { bootstrapSignedIn } from './bootstrap';
import { load, projects } from './stores.svelte';
import type { ProjectSummary } from './types';

const summary = (over: Partial<ProjectSummary>): ProjectSummary => ({
  project: 'p',
  issue_count: 0,
  ...over
});

beforeEach(() => {
  projects.available = [];
  projects.current = 'default';
  load.initialLoad = true;
  connectMock.mockReset();
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('bootstrapSignedIn', () => {
  it('populates projects.available and connects to the first project', async () => {
    const summaries = [summary({ project: 'foo' }), summary({ project: 'bar' })];
    vi.spyOn(api, 'projects').mockResolvedValue(summaries);

    await bootstrapSignedIn();

    expect(projects.available).toEqual(summaries);
    expect(connectMock).toHaveBeenCalledWith('foo');
  });

  it('connects to projects.current when the daemon has zero projects', async () => {
    // Fresh-install path: api.projects() resolves with [], so the bootstrap
    // still has to bring up the WS so the eventual snapshot can flip
    // load.initialLoad → false.
    vi.spyOn(api, 'projects').mockResolvedValue([]);
    projects.current = 'default';

    await bootstrapSignedIn();

    expect(projects.available).toEqual([]);
    expect(connectMock).toHaveBeenCalledWith('default');
  });

  it('falls back to projects.current when /api/projects fails', async () => {
    // Daemon transient error must not leave the UI un-connected — the WS
    // still comes up against the last known project, and the user sees the
    // toast surface the failure.
    vi.spyOn(api, 'projects').mockRejectedValue(new Error('network'));
    projects.current = 'staging';

    await bootstrapSignedIn();

    expect(connectMock).toHaveBeenCalledWith('staging');
  });

  it('clears load.initialLoad after the 4s fallback', async () => {
    // The WS snapshot is the primary signal that flips initialLoad; the
    // setTimeout is a safety net for daemons that never deliver a snapshot.
    vi.spyOn(api, 'projects').mockResolvedValue([]);

    await bootstrapSignedIn();

    expect(load.initialLoad).toBe(true);
    vi.advanceTimersByTime(4_000);
    expect(load.initialLoad).toBe(false);
  });
});
