import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { retention } from './retention.svelte';

afterEach(() => {
  vi.restoreAllMocks();
  // Reset singleton state so tests don't bleed.
  retention.current = { events_per_issue_max: 0, issues_per_project_max: 0, event_retention_days: 0 };
  retention.draft = { events_per_issue_max: 0, issues_per_project_max: 0, event_retention_days: 0 };
  retention.error = null;
  retention.saving = false;
  retention.loading = false;
});

function mockOk(json: unknown) {
  return vi.spyOn(globalThis, 'fetch').mockResolvedValue({
    ok: true,
    status: 200,
    headers: { get: () => 'application/json' } as unknown as Headers,
    json: async () => json,
    text: async () => ''
  } as Response);
}

function mockErr(status: number, body = '') {
  return vi.spyOn(globalThis, 'fetch').mockResolvedValue({
    ok: false,
    status,
    headers: { get: () => null } as unknown as Headers,
    json: async () => ({}),
    text: async () => body
  } as Response);
}

describe('retention store', () => {
  it('load fills current and draft', async () => {
    mockOk({ events_per_issue_max: 50, issues_per_project_max: 1000, event_retention_days: 7 });
    await retention.load();
    expect(retention.current.events_per_issue_max).toBe(50);
    expect(retention.draft.event_retention_days).toBe(7);
    expect(retention.error).toBeNull();
  });

  it('dirty flips when draft differs from current', async () => {
    mockOk({ events_per_issue_max: 0, issues_per_project_max: 0, event_retention_days: 0 });
    await retention.load();
    expect(retention.dirty).toBe(false);
    retention.draft.event_retention_days = 14;
    expect(retention.dirty).toBe(true);
    retention.reset();
    expect(retention.dirty).toBe(false);
  });

  it('save round-trips and clears dirty', async () => {
    mockOk({ events_per_issue_max: 0, issues_per_project_max: 0, event_retention_days: 0 });
    await retention.load();
    retention.draft.events_per_issue_max = 50;
    expect(retention.dirty).toBe(true);
    mockOk({ events_per_issue_max: 50, issues_per_project_max: 0, event_retention_days: 0 });
    const ok = await retention.save();
    expect(ok).toBe(true);
    expect(retention.current.events_per_issue_max).toBe(50);
    expect(retention.dirty).toBe(false);
  });

  it('save rejects negative values without hitting the network', async () => {
    retention.draft = { events_per_issue_max: -1, issues_per_project_max: 0, event_retention_days: 0 };
    const fetch = vi.spyOn(globalThis, 'fetch');
    const ok = await retention.save();
    expect(ok).toBe(false);
    expect(retention.error).toBe('invalid');
    expect(fetch).not.toHaveBeenCalled();
  });

  it('save surfaces 403 as forbidden', async () => {
    retention.draft = { events_per_issue_max: 1, issues_per_project_max: 0, event_retention_days: 0 };
    mockErr(403, 'admin only');
    const ok = await retention.save();
    expect(ok).toBe(false);
    expect(retention.error).toBe('forbidden');
  });

  it('save surfaces 400 as invalid', async () => {
    retention.draft = { events_per_issue_max: 1, issues_per_project_max: 0, event_retention_days: 0 };
    mockErr(400, 'values must be >= 0');
    const ok = await retention.save();
    expect(ok).toBe(false);
    expect(retention.error).toBe('invalid');
  });
});
