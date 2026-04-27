// Local-only assignee tracking.
//
// Status used to live here (localStorage) but is now server-authoritative
// — see `api.setStatus` and `Issue.status`. Assignee remains local because
// the daemon's wire `Issue` has no `assignee` field; a future proto bump
// can move it to the server, at which point this module evaporates.
//
// Persisted by fingerprint (stable across daemon restarts) rather than id.

import type { Issue } from './types';

const STORAGE_KEY = 'errex.assignees.v2';
const ME_KEY = 'errex.me.v1';

export interface LocalAssignee {
  assignee: string | null;
  updatedAt: number;
}

class ActionsStore {
  byFingerprint = $state<Map<string, LocalAssignee>>(new Map());
  /** The "you" identity for assign-to-me. Free-text; first-run defaults to "me". */
  me = $state<string>('me');

  hydrate() {
    if (typeof localStorage === 'undefined') return;
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (raw) {
        const parsed = JSON.parse(raw) as Record<string, LocalAssignee>;
        this.byFingerprint = new Map(Object.entries(parsed));
      }
      const me = localStorage.getItem(ME_KEY);
      if (me) this.me = me;
    } catch {
      // Corrupt storage shouldn't crash the app; ignore and start clean.
    }
  }

  private persist() {
    if (typeof localStorage === 'undefined') return;
    const obj: Record<string, LocalAssignee> = {};
    for (const [k, v] of this.byFingerprint) obj[k] = v;
    localStorage.setItem(STORAGE_KEY, JSON.stringify(obj));
  }

  setMe(name: string) {
    this.me = name.trim() || 'me';
    if (typeof localStorage !== 'undefined') localStorage.setItem(ME_KEY, this.me);
  }

  /** Returns null when no local assignee is set for this issue. */
  assigneeFor(issue: Issue): string | null {
    return this.byFingerprint.get(issue.fingerprint)?.assignee ?? null;
  }

  /** Returns the previous value for Undo. */
  setAssignee(issue: Issue, assignee: string | null): string | null {
    const prev = this.assigneeFor(issue);
    const m = new Map(this.byFingerprint);
    if (assignee === null) {
      m.delete(issue.fingerprint);
    } else {
      m.set(issue.fingerprint, { assignee, updatedAt: Date.now() });
    }
    this.byFingerprint = m;
    this.persist();
    return prev;
  }

  assignToMe(issue: Issue): string | null {
    return this.setAssignee(issue, this.me);
  }
  unassign(issue: Issue): string | null {
    return this.setAssignee(issue, null);
  }
}

export const actions = new ActionsStore();
