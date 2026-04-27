// Team (user management) state.
//
// Mirrors the cached user list and exposes the mutators the team page
// needs. Auth is via the global session cookie — see `auth.svelte.ts`.

import { api, HttpError, type Role, type User, type UserSession } from './api';

export type TeamError = 'unauthorized' | 'forbidden' | 'network' | null;

class TeamStore {
  users = $state<User[]>([]);
  loading = $state(false);
  error = $state<TeamError>(null);

  async loadUsers(): Promise<void> {
    this.loading = true;
    this.error = null;
    try {
      this.users = await api.admin.listUsers();
    } catch (err) {
      this.error = mapError(err);
    } finally {
      this.loading = false;
    }
  }

  async createUser(username: string, password: string, role: Role): Promise<User> {
    const created = await api.admin.createUser(username, password, role);
    await this.loadUsers();
    return created;
  }

  async updateUser(
    username: string,
    patch: { password?: string; role?: Role; deactivated?: boolean }
  ): Promise<User> {
    const updated = await api.admin.updateUser(username, patch);
    await this.loadUsers();
    return updated;
  }

  async deleteUser(username: string): Promise<void> {
    await api.admin.deleteUser(username);
    await this.loadUsers();
  }

  /** Read-only — does not mutate the cached list. The detail panel reloads
   *  this whenever it needs to render the active session table. */
  listUserSessions(username: string): Promise<UserSession[]> {
    return api.admin.listUserSessions(username);
  }

  async revokeUserSessions(username: string): Promise<number> {
    const r = await api.admin.revokeUserSessions(username);
    return r.sessions_revoked;
  }
}

function mapError(err: unknown): TeamError {
  if (err instanceof HttpError) {
    if (err.status === 401) return 'unauthorized';
    if (err.status === 403) return 'forbidden';
  }
  return 'network';
}

export const team = new TeamStore();
