// Retention settings store.
//
// Wraps `api.admin.getRetention` / `setRetention` with rune-driven
// state for the settings page. The form binds to `draft`, the saved
// snapshot lives in `current`, and `dirty` is derived. Saving
// resyncs both.

import { api, HttpError, type RetentionSettings, type StorageStats } from './api';

export type RetentionError = 'unauthorized' | 'forbidden' | 'invalid' | 'network' | null;

const ZERO: RetentionSettings = {
  events_per_issue_max: 0,
  issues_per_project_max: 0,
  event_retention_days: 0
};

class RetentionStore {
  /** Last value persisted server-side (or initial defaults before load). */
  current = $state<RetentionSettings>({ ...ZERO });
  /** Form-bound editable copy. */
  draft = $state<RetentionSettings>({ ...ZERO });
  /** Snapshot of the daemon's current storage state. `null` until loaded. */
  stats = $state<StorageStats | null>(null);
  loading = $state(false);
  saving = $state(false);
  error = $state<RetentionError>(null);

  /** True when the form has unsaved edits. */
  get dirty(): boolean {
    return (
      this.draft.events_per_issue_max !== this.current.events_per_issue_max ||
      this.draft.issues_per_project_max !== this.current.issues_per_project_max ||
      this.draft.event_retention_days !== this.current.event_retention_days
    );
  }

  /** How many of the three retention limits are actively bounded
   *  (non-zero) on the draft. Drives the "{n}/3 active" header chip. */
  get activeLimitCount(): number {
    let n = 0;
    if (this.draft.events_per_issue_max > 0) n++;
    if (this.draft.issues_per_project_max > 0) n++;
    if (this.draft.event_retention_days > 0) n++;
    return n;
  }

  async load(): Promise<void> {
    this.loading = true;
    this.error = null;
    try {
      const s = await api.admin.getRetention();
      this.current = { ...s };
      this.draft = { ...s };
    } catch (err) {
      this.error = mapError(err);
    } finally {
      this.loading = false;
    }
  }

  /** Pull the storage snapshot. Independent from `load()` so the page can
   *  fetch both in parallel and so a stats failure doesn't block the form. */
  async loadStats(): Promise<void> {
    try {
      const s = await api.admin.getStorage();
      this.stats = s;
    } catch (err) {
      // Surface the auth-relevant failures through the same `error` field
      // so the page-level toast can react. Network blips just leave stats
      // as `null` — the header degrades to "—" instead of crashing.
      const mapped = mapError(err);
      if (mapped === 'unauthorized' || mapped === 'forbidden') {
        this.error = mapped;
      }
    }
  }

  async save(): Promise<boolean> {
    if (!this.isDraftValid()) {
      this.error = 'invalid';
      return false;
    }
    this.saving = true;
    this.error = null;
    try {
      const s = await api.admin.setRetention({ ...this.draft });
      this.current = { ...s };
      this.draft = { ...s };
      return true;
    } catch (err) {
      this.error = mapError(err);
      return false;
    } finally {
      this.saving = false;
    }
  }

  reset(): void {
    this.draft = { ...this.current };
    this.error = null;
  }

  /** All three fields must be non-negative integers. The daemon enforces
   *  this server-side too (returns 400 on negatives) — checking client-side
   *  is purely so the user gets immediate feedback before sending. */
  isDraftValid(): boolean {
    const { events_per_issue_max, issues_per_project_max, event_retention_days } = this.draft;
    return (
      Number.isInteger(events_per_issue_max) &&
      Number.isInteger(issues_per_project_max) &&
      Number.isInteger(event_retention_days) &&
      events_per_issue_max >= 0 &&
      issues_per_project_max >= 0 &&
      event_retention_days >= 0
    );
  }
}

function mapError(err: unknown): RetentionError {
  if (err instanceof HttpError) {
    if (err.status === 401) return 'unauthorized';
    if (err.status === 403) return 'forbidden';
    if (err.status === 400) return 'invalid';
  }
  return 'network';
}

export const retention = new RetentionStore();
