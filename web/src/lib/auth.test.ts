import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { auth } from './auth.svelte';
import { api, HttpError, type AuthMe } from './api';

const someone: AuthMe = { username: 'daisy', role: 'admin' };

beforeEach(() => {
  auth.user = null;
  auth.status = 'unknown';
  auth.lockoutUntilEpoch = 0;
  auth.needsSetup = false;
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('auth.hydrate', () => {
  it('sets signed_in + user when /me returns 200', async () => {
    vi.spyOn(api.auth, 'me').mockResolvedValue(someone);
    await auth.hydrate();
    expect(auth.status).toBe('signed_in');
    expect(auth.user).toEqual(someone);
    expect(auth.needsSetup).toBe(false);
  });

  it('sets signed_out when /me returns 401', async () => {
    vi.spyOn(api.auth, 'me').mockRejectedValue(new HttpError(401, 'unauth'));
    vi.spyOn(api.auth, 'setupStatus').mockResolvedValue({
      needs_setup: false,
      setup_disabled: false
    });
    await auth.hydrate();
    expect(auth.status).toBe('signed_out');
    expect(auth.user).toBeNull();
  });

  it('treats unexpected failures as signed_out so the UI can recover', async () => {
    vi.spyOn(api.auth, 'me').mockRejectedValue(new Error('network'));
    vi.spyOn(api.auth, 'setupStatus').mockRejectedValue(new Error('network'));
    await auth.hydrate();
    expect(auth.status).toBe('signed_out');
  });

  it('flags needsSetup when the daemon is brand-new (zero users + token configured)', async () => {
    vi.spyOn(api.auth, 'me').mockRejectedValue(new HttpError(401, 'unauth'));
    vi.spyOn(api.auth, 'setupStatus').mockResolvedValue({
      needs_setup: true,
      setup_disabled: false
    });
    await auth.hydrate();
    expect(auth.status).toBe('signed_out');
    expect(auth.needsSetup).toBe(true);
  });

  it('does not flag needsSetup when setup-status itself fails', async () => {
    // Daemon unreachable for both endpoints — the layout should fall back
    // to /login rather than route to a /setup that may not even respond.
    vi.spyOn(api.auth, 'me').mockRejectedValue(new Error('network'));
    vi.spyOn(api.auth, 'setupStatus').mockRejectedValue(new Error('network'));
    await auth.hydrate();
    expect(auth.needsSetup).toBe(false);
  });
});

describe('auth.login', () => {
  it('populates user state and clears lockout', async () => {
    auth.lockoutUntilEpoch = 9999999999;
    vi.spyOn(api.auth, 'login').mockResolvedValue(someone);
    const me = await auth.login('daisy', 'pw');
    expect(me).toEqual(someone);
    expect(auth.user).toEqual(someone);
    expect(auth.status).toBe('signed_in');
    expect(auth.lockoutUntilEpoch).toBe(0);
  });

  it('captures Retry-After on 429 so the form can countdown', async () => {
    const err = new HttpError(429, 'too many');
    err.retryAfterSecs = 60;
    vi.spyOn(api.auth, 'login').mockRejectedValue(err);
    const before = Date.now();
    await expect(auth.login('daisy', 'wrong')).rejects.toBe(err);
    expect(auth.lockoutUntilEpoch).toBeGreaterThanOrEqual(before + 59_000);
    expect(auth.status).not.toBe('signed_in');
  });

  it('rethrows non-429 errors without setting lockout', async () => {
    vi.spyOn(api.auth, 'login').mockRejectedValue(new HttpError(401, 'wrong creds'));
    await expect(auth.login('daisy', 'x')).rejects.toBeInstanceOf(HttpError);
    expect(auth.lockoutUntilEpoch).toBe(0);
  });
});

describe('auth.setup', () => {
  it('signs the user in immediately on success', async () => {
    vi.spyOn(api.auth, 'setup').mockResolvedValue(someone);
    await auth.setup('the-token', 'daisy', 'a-strong-passphrase');
    expect(auth.status).toBe('signed_in');
    expect(auth.user).toEqual(someone);
  });
});

describe('auth.logout', () => {
  it('clears local state on success', async () => {
    auth.user = someone;
    auth.status = 'signed_in';
    vi.spyOn(api.auth, 'logout').mockResolvedValue(undefined as unknown as void);
    await auth.logout();
    expect(auth.user).toBeNull();
    expect(auth.status).toBe('signed_out');
  });

  it('still clears local state if the server call fails (defensive)', async () => {
    auth.user = someone;
    auth.status = 'signed_in';
    vi.spyOn(api.auth, 'logout').mockRejectedValue(new Error('network'));
    await auth.logout();
    expect(auth.user).toBeNull();
    expect(auth.status).toBe('signed_out');
  });
});

describe('auth.isAdmin', () => {
  it('is true when the user is admin', () => {
    auth.user = { username: 'a', role: 'admin' };
    expect(auth.isAdmin()).toBe(true);
  });
  it('is false when viewer', () => {
    auth.user = { username: 'a', role: 'viewer' };
    expect(auth.isAdmin()).toBe(false);
  });
  it('is false when unauthed', () => {
    auth.user = null;
    expect(auth.isAdmin()).toBe(false);
  });
});
