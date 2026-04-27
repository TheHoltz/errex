import { describe, expect, it } from 'vitest';
import {
  buildTestEventCurl,
  formatWebhookHealth,
  isDeleteConfirmed,
  projectActivityStatus,
  validateNewProjectName
} from './projectsConsole';

const NOW = new Date('2026-04-26T12:00:00Z').getTime();

describe('projectActivityStatus', () => {
  it('returns "never used" when last_used_at is null', () => {
    const s = projectActivityStatus(null, NOW);
    expect(s.label).toBe('never used');
    expect(s.tone).toContain('muted-foreground');
  });

  it('returns "live" within 5 minutes', () => {
    const at = new Date(NOW - 30_000).toISOString();
    expect(projectActivityStatus(at, NOW).label).toBe('live');
  });

  it('returns "recent" within 1 hour', () => {
    const at = new Date(NOW - 30 * 60_000).toISOString();
    expect(projectActivityStatus(at, NOW).label).toBe('recent');
  });

  it('returns "idle" beyond 1 hour', () => {
    const at = new Date(NOW - 4 * 3_600_000).toISOString();
    expect(projectActivityStatus(at, NOW).label).toBe('idle');
  });

  it('treats undefined like null', () => {
    expect(projectActivityStatus(undefined, NOW).label).toBe('never used');
  });
});

describe('formatWebhookHealth', () => {
  it('returns "never delivered" when status is null', () => {
    expect(formatWebhookHealth(null, null, NOW)).toEqual({
      tone: 'never',
      label: 'never delivered'
    });
  });

  it('flags 2xx as ok with relative time', () => {
    const at = new Date(NOW - 12_000).toISOString();
    const h = formatWebhookHealth(200, at, NOW);
    expect(h.tone).toBe('ok');
    expect(h.label).toMatch(/200/);
    expect(h.label).toMatch(/12s ago/);
  });

  it('flags 4xx and 5xx as fail with the actual status', () => {
    const at = new Date(NOW - 60_000).toISOString();
    expect(formatWebhookHealth(404, at, NOW)).toMatchObject({ tone: 'fail' });
    expect(formatWebhookHealth(502, at, NOW)).toMatchObject({ tone: 'fail' });
    expect(formatWebhookHealth(502, at, NOW).label).toMatch(/502/);
  });

  it('translates 0 to "transport error"', () => {
    const at = new Date(NOW - 5_000).toISOString();
    const h = formatWebhookHealth(0, at, NOW);
    expect(h.tone).toBe('fail');
    expect(h.label).toMatch(/transport/);
  });

  it('formats older deliveries in minutes / hours / days', () => {
    expect(formatWebhookHealth(200, new Date(NOW - 90_000).toISOString(), NOW).label).toMatch(
      /m ago/
    );
    expect(
      formatWebhookHealth(200, new Date(NOW - 5 * 3_600_000).toISOString(), NOW).label
    ).toMatch(/h ago/);
    expect(
      formatWebhookHealth(200, new Date(NOW - 3 * 24 * 3_600_000).toISOString(), NOW).label
    ).toMatch(/d ago/);
  });
});

describe('isDeleteConfirmed', () => {
  it('matches when typed name equals project name', () => {
    expect(isDeleteConfirmed('checkout-api', 'checkout-api')).toBe(true);
  });

  it('trims surrounding whitespace before comparing', () => {
    expect(isDeleteConfirmed('  checkout-api  ', 'checkout-api')).toBe(true);
  });

  it('is case sensitive (project names are case sensitive)', () => {
    expect(isDeleteConfirmed('checkout-api', 'Checkout-API')).toBe(false);
  });

  it('rejects empty input even when project name is empty (defense in depth)', () => {
    expect(isDeleteConfirmed('', '')).toBe(false);
    expect(isDeleteConfirmed('   ', 'anything')).toBe(false);
  });
});

describe('buildTestEventCurl', () => {
  const DSN = 'http://localhost:9090/api/my-app/envelope/?sentry_key=tok123';
  const FIXED = {
    eventId: '00000000-0000-0000-0000-000000000001',
    sentAt: '2026-04-26T12:00:00Z'
  };

  it('targets the DSN URL with POST', () => {
    const cmd = buildTestEventCurl(DSN, FIXED);
    expect(cmd).toMatch(/^curl -X POST '/);
    expect(cmd).toContain(`'${DSN}'`);
  });

  it('sets the Sentry envelope content-type', () => {
    const cmd = buildTestEventCurl(DSN, FIXED);
    expect(cmd).toContain("-H 'content-type: application/x-sentry-envelope'");
  });

  it('uses --data-binary with $\'…\' so \\n stays literal in the body', () => {
    // Plain `-d` collapses newlines on some shells; the envelope parser
    // requires real newlines between header / item-header / payload.
    const cmd = buildTestEventCurl(DSN, FIXED);
    expect(cmd).toMatch(/--data-binary \$'/);
  });

  function extractBody(cmd: string): string {
    const match = cmd.match(/--data-binary \$'([^']*)'/);
    if (!match || match[1] === undefined) {
      throw new Error(`expected --data-binary $'…' in: ${cmd}`);
    }
    return match[1];
  }

  it('emits exactly three newline-separated JSON lines: envelope header, item header, payload', () => {
    const body = extractBody(buildTestEventCurl(DSN, FIXED));
    // Trailing \n after the payload is part of the wire format. Splitting on \n
    // therefore yields four parts where the last one is empty.
    const parts = body.split('\\n');
    expect(parts).toHaveLength(4);
    expect(parts[3]).toBe('');

    const envHeader = JSON.parse(parts[0] ?? '');
    expect(envHeader.event_id).toBe(FIXED.eventId);
    expect(envHeader.sent_at).toBe(FIXED.sentAt);

    const itemHeader = JSON.parse(parts[1] ?? '');
    expect(itemHeader.type).toBe('event');

    const payload = JSON.parse(parts[2] ?? '');
    expect(payload.level).toBe('error');
    expect(payload.message).toMatch(/errex/i);
    expect(payload.exception?.values?.[0]?.type).toBeDefined();
  });

  it('defaults eventId / sentAt when not provided (helper is callable in production)', () => {
    const body = extractBody(buildTestEventCurl(DSN));
    const envHeader = JSON.parse(body.split('\\n')[0] ?? '');
    expect(envHeader.event_id).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i
    );
    expect(() => new Date(envHeader.sent_at).toISOString()).not.toThrow();
  });
});

describe('validateNewProjectName', () => {
  it('accepts a valid new name', () => {
    expect(validateNewProjectName('new-name', 'old-name')).toEqual({ ok: true });
  });

  it('rejects empty', () => {
    const r = validateNewProjectName('   ', 'old');
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.reason).toMatch(/required/);
  });

  it('rejects names over 64 characters', () => {
    const r = validateNewProjectName('a'.repeat(65), 'old');
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.reason).toMatch(/too long/);
  });

  it('rejects unchanged name (nothing to do)', () => {
    const r = validateNewProjectName('same', 'same');
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.reason).toMatch(/unchanged/);
  });
});
