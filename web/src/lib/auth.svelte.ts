// Current-user / session state.
//
// The browser holds an opaque `errex_session` cookie minted by the daemon
// on `/api/auth/login` (or `/api/auth/setup`). All `api.*` calls send it
// automatically via same-origin credentials; this store just remembers
// who the daemon says we are so components can render "Signed in as foo
// (admin)" without re-fetching.
//
// Bootstrap flow on page load:
//   1. fetch /api/auth/setup-status → decide /setup vs /login
//   2. if signed in (cookie still valid) /api/auth/me → populate `user`
//   3. otherwise the root layout sends the operator to /login
//
// Lockout (HTTP 429) state is exposed so the login form can show a
// countdown without each component re-implementing the timer.

import { api, HttpError, type AuthMe } from './api';

export type AuthStatus = 'unknown' | 'signed_in' | 'signed_out';

class AuthStore {
  status = $state<AuthStatus>('unknown');
  user = $state<AuthMe | null>(null);
  /** Seconds remaining on a per-account lockout, or 0 when unlocked. */
  lockoutUntilEpoch = $state<number>(0);
  /** True iff the daemon has zero users AND a setup token is configured.
   *  Lets the root layout route signed-out visitors directly to /setup
   *  instead of flashing them through /login first. Snapshot taken at
   *  hydrate time; cleared on successful login/setup. */
  needsSetup = $state<boolean>(false);

  /** Probe the server. Sets `status` to `signed_in` or `signed_out` based
   *  on whether the cookie resolved to a valid session. When signed_out,
   *  also probes setup-status so the layout can pick the right pre-auth
   *  destination on the first paint. */
  async hydrate(): Promise<void> {
    try {
      this.user = await api.auth.me();
      this.status = 'signed_in';
      this.needsSetup = false;
    } catch (err) {
      // 401 is the expected "not signed in" path; everything else (network
      // hiccup) we also treat as signed_out so the SPA shows /login. The
      // alternative — leaving status='unknown' — would freeze the UI on a
      // loading spinner during transient failures.
      this.user = null;
      this.status = 'signed_out';
      if (!(err instanceof HttpError && err.status === 401)) {
        // Surface unexpected errors via console so debugging is possible.
        console.warn('auth.hydrate failed', err);
      }
      // Probe setup-status so the layout can route signed-out users
      // directly to /setup when the daemon is brand-new. Failure here is
      // benign — needsSetup stays false and the layout falls back to
      // /login.
      try {
        const s = await api.auth.setupStatus();
        this.needsSetup = s.needs_setup;
      } catch {
        this.needsSetup = false;
      }
    }
  }

  /** Sign in. On 429, populates `lockoutUntilEpoch` so the form can show
   *  a countdown. The HttpError is rethrown so the caller can render the
   *  exact message. */
  async login(username: string, password: string): Promise<AuthMe> {
    try {
      const me = await api.auth.login(username, password);
      this.user = me;
      this.status = 'signed_in';
      this.lockoutUntilEpoch = 0;
      this.needsSetup = false;
      return me;
    } catch (err) {
      if (err instanceof HttpError && err.status === 429 && err.retryAfterSecs) {
        this.lockoutUntilEpoch = Date.now() + err.retryAfterSecs * 1000;
      }
      throw err;
    }
  }

  /** First-user creation through the onboarding wizard. Same shape as
   *  login on success — the server signs the operator in immediately so
   *  there's no awkward "now log back in" step. */
  async setup(token: string, username: string, password: string): Promise<AuthMe> {
    const me = await api.auth.setup(token, username, password);
    this.user = me;
    this.status = 'signed_in';
    this.lockoutUntilEpoch = 0;
    this.needsSetup = false;
    return me;
  }

  async logout(): Promise<void> {
    try {
      await api.auth.logout();
    } catch {
      // Even if the server didn't acknowledge (network drop), drop local
      // state so the user sees the signed-out UI. Worst case the cookie
      // sticks around and the next call gets a 401, which we handle.
    }
    this.user = null;
    this.status = 'signed_out';
  }

  isAdmin(): boolean {
    return this.user?.role === 'admin';
  }
}

export const auth = new AuthStore();
