// Admin (project management) state.
//
// Mirrors the cached project list and exposes the mutators the SPA needs.
// Authentication is the global `errex_session` cookie (set by `/api/auth/login`)
// — this store no longer holds a bearer token. If a call returns 401/403, the
// caller is responsible for routing to /login; we just propagate the error.
//
// `loadProjects()` is idempotent and bumped after every mutation.

import {
  api,
  HttpError,
  type ActivityStats,
  type AdminProject,
  type DeleteSummary
} from './api';
import { projects } from './stores.svelte';

export type AdminError = 'unauthorized' | 'forbidden' | 'network' | null;

class AdminStore {
  /** Cached admin project list — mirrors what the SPA settings modal shows. */
  projects = $state<AdminProject[]>([]);
  loading = $state(false);
  error = $state<AdminError>(null);

  /** Load (or refresh) the admin project list. Surfaces the error state.
   *  Also refreshes the role-neutral `projects.available` store so the
   *  first-run gate, project switcher, and command palette see fresh data
   *  after every admin mutation (each mutation calls this method, so the
   *  two stores stay locked through a single write site). */
  async loadProjects(): Promise<void> {
    this.loading = true;
    this.error = null;
    try {
      this.projects = await api.admin.listProjects();
      projects.available = await api.projects();
    } catch (err) {
      this.error = mapError(err);
    } finally {
      this.loading = false;
    }
  }

  /** Create a project; on success, refresh the cache and return the row. */
  async createProject(name: string): Promise<AdminProject> {
    const created = await api.admin.createProject(name);
    await this.loadProjects();
    return created;
  }

  async setWebhook(name: string, url: string | null): Promise<void> {
    await api.admin.setWebhook(name, url);
    await this.loadProjects();
  }

  async rotateToken(name: string): Promise<AdminProject> {
    const rotated = await api.admin.rotateToken(name);
    await this.loadProjects();
    return rotated;
  }

  async renameProject(oldName: string, newName: string): Promise<AdminProject> {
    const renamed = await api.admin.renameProject(oldName, newName);
    await this.loadProjects();
    return renamed;
  }

  async deleteProject(name: string): Promise<DeleteSummary> {
    const summary = await api.admin.deleteProject(name);
    await this.loadProjects();
    return summary;
  }

  /** Read-only — does not mutate the cached list. The console uses this to
   *  populate the type-to-confirm modal copy before the user types. */
  destroyPreview(name: string): Promise<DeleteSummary> {
    return api.admin.destroyPreview(name);
  }

  /** 24h activity rollup for a single project. The console refetches this
   *  every time a fresh event arrives over WS so the sparkline stays live. */
  getActivity(name: string): Promise<ActivityStats> {
    return api.admin.getActivity(name);
  }

  /** Bump cached `last_used_at` in place when a fresh event lands over WS.
   *  Avoids waiting for the next periodic refresh to clear "no events yet". */
  bumpUsed(name: string, when: string): void {
    const idx = this.projects.findIndex((p) => p.name === name);
    if (idx === -1) return;
    const cur = this.projects[idx]!;
    if (cur.last_used_at && Date.parse(cur.last_used_at) >= Date.parse(when)) return;
    this.projects[idx] = { ...cur, last_used_at: when };
  }
}

function mapError(err: unknown): AdminError {
  if (err instanceof HttpError) {
    if (err.status === 401) return 'unauthorized';
    if (err.status === 403) return 'forbidden';
  }
  return 'network';
}

export const admin = new AdminStore();
