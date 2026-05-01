// ─────────────────────────────────────────────────────────────────────
//  Saved filters & recents — both backed by localStorage so they
//  persist per-browser. Two stores intentionally; saved filters are
//  intentional and named, recents are automatic and disposable.
// ─────────────────────────────────────────────────────────────────────

import { browser } from '$app/environment';

export type SavedFilter = { name: string; query: string };

const SAVED_KEY = 'errex.savedFilters';
const RECENTS_KEY = 'errex.recentQueries';
const RECENTS_MAX = 8;

function readSaved(): SavedFilter[] {
  if (!browser) return [];
  try {
    const raw = localStorage.getItem(SAVED_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    // Defensive: drop anything that doesn't have the expected shape
    // rather than crash later when the user tries to recall.
    return parsed.filter(
      (e): e is SavedFilter =>
        typeof e === 'object' && e != null && typeof e.name === 'string' && typeof e.query === 'string'
    );
  } catch {
    return [];
  }
}

function readRecents(): string[] {
  if (!browser) return [];
  try {
    const raw = localStorage.getItem(RECENTS_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((s): s is string => typeof s === 'string').slice(0, RECENTS_MAX);
  } catch {
    return [];
  }
}

class SavedFiltersStore {
  list = $state<SavedFilter[]>(readSaved());

  // Each mutation persists synchronously so a reload always sees the
  // latest state. The cost is negligible for a list this small.
  save(name: string, query: string) {
    const existing = this.list.findIndex((s) => s.name === name);
    if (existing >= 0) {
      const next = [...this.list];
      next[existing] = { name, query };
      this.list = next;
    } else {
      this.list = [...this.list, { name, query }];
    }
    this.persist();
  }

  remove(name: string) {
    this.list = this.list.filter((s) => s.name !== name);
    this.persist();
  }

  find(name: string): SavedFilter | undefined {
    return this.list.find((s) => s.name === name);
  }

  private persist() {
    if (!browser) return;
    try {
      localStorage.setItem(SAVED_KEY, JSON.stringify(this.list));
    } catch {
      // Quota or privacy mode — silently ignore; the in-memory state
      // still works for the rest of this session.
    }
  }
}

class RecentsStore {
  list = $state<string[]>(readRecents());

  push(query: string) {
    const trimmed = query.trim();
    if (trimmed.length === 0) return;
    const without = this.list.filter((q) => q !== trimmed);
    this.list = [trimmed, ...without].slice(0, RECENTS_MAX);
    this.persist();
  }

  clear() {
    this.list = [];
    this.persist();
  }

  private persist() {
    if (!browser) return;
    try {
      localStorage.setItem(RECENTS_KEY, JSON.stringify(this.list));
    } catch {
      // ditto
    }
  }
}

export const savedFilters = new SavedFiltersStore();
export const recents = new RecentsStore();
