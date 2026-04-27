// Build a markdown bundle that captures an issue + its latest event in a
// shape ready to paste into an external LLM. Goal: the AI receives every
// concrete signal a human triager would have — exception, stack, breadcrumb
// timeline, environment, user, request, and the raw payload as a fallback
// for fields the formatter doesn't surface explicitly.

import type { Frame, Issue, NormalizedEvent } from './types';

type Json = unknown;

export function formatIssueContext(issue: Issue, event: NormalizedEvent | null): string {
  const sections: string[] = [];

  sections.push(header(issue, event));

  if (!event) {
    sections.push('No event payload available — issue exists but no stored event yet.');
    return sections.join('\n\n');
  }

  const payload = (event.raw.payload ?? {}) as Record<string, Json>;

  if (event.exception) sections.push(stackSection(event.exception.type, event.exception.value, event.exception.frames));
  if (event.breadcrumbs.length > 0) sections.push(breadcrumbSection(event.breadcrumbs));
  if (Object.keys(event.tags).length > 0) sections.push(tagsSection(event.tags));

  const ctx = contextSection(payload, event);
  if (ctx) sections.push(ctx);

  const usr = userSection(payload['user']);
  if (usr) sections.push(usr);

  const req = requestSection(payload['request']);
  if (req) sections.push(req);

  sections.push(rawSection(event.raw));

  return sections.join('\n\n');
}

function header(issue: Issue, event: NormalizedEvent | null): string {
  const lines = [
    `# ${issue.title}`,
    '',
    `- Project: ${issue.project}`,
    `- Status: ${issue.status}`,
    `- Level: ${issue.level ?? 'unknown'}`,
    `- Fingerprint: ${issue.fingerprint}`,
    `- Event count: ${issue.event_count}`,
    `- First seen: ${issue.first_seen}`,
    `- Last seen: ${issue.last_seen}`
  ];
  if (issue.culprit) lines.push(`- Culprit: ${issue.culprit}`);
  if (event?.release) lines.push(`- Release: ${event.release}`);
  if (event?.environment) lines.push(`- Environment: ${event.environment}`);
  if (event?.received_at) lines.push(`- Received: ${event.received_at}`);
  return lines.join('\n');
}

function stackSection(ty: string | null, value: string | null, frames: Frame[]): string {
  const head = ty || value ? [`## Exception`, `**${ty ?? 'Error'}**${value ? `: ${value}` : ''}`].join('\n\n') : '## Exception';
  if (frames.length === 0) return head;
  // Sentry convention: oldest frame first; the most-relevant (innermost) is
  // last. Number them so the AI can refer to specific frames by index.
  const lines = frames.map((f, i) => formatFrame(f, i + 1));
  return `${head}\n\n## Stack trace\n${lines.join('\n')}`;
}

function formatFrame(f: Frame, n: number): string {
  const fn = f.function ?? '<anonymous>';
  const file = f.filename ?? '<unknown>';
  const loc = f.lineno != null ? `:${f.lineno}${f.colno != null ? `:${f.colno}` : ''}` : '';
  const flags = f.in_app ? ' in_app' : '';
  return `${n}. \`${fn}\` — ${file}${loc}${flags}`;
}

function breadcrumbSection(bcs: NormalizedEvent['breadcrumbs']): string {
  const lines = bcs.map((b) => {
    const ts = b.timestamp ?? '—';
    const cat = b.category ?? 'event';
    const lvl = b.level ? `/${b.level}` : '';
    const msg = b.message ?? '';
    return `- ${ts} [${cat}${lvl}] ${msg}`.trimEnd();
  });
  return `## Breadcrumbs (${bcs.length})\n${lines.join('\n')}`;
}

function tagsSection(tags: Record<string, string>): string {
  const lines = Object.entries(tags).map(([k, v]) => `- ${k}: ${v}`);
  return `## Tags\n${lines.join('\n')}`;
}

function contextSection(payload: Record<string, Json>, event: NormalizedEvent): string | null {
  const ctxs = (payload['contexts'] ?? null) as Record<string, Record<string, Json>> | null;
  const lines: string[] = [];
  if (ctxs) {
    const os = ctxs['os'];
    const br = ctxs['browser'];
    const dev = ctxs['device'];
    const rt = ctxs['runtime'];
    const app = ctxs['app'];
    if (os) lines.push(`- OS: ${stringifyContext(os)}`);
    if (br) lines.push(`- Browser: ${stringifyContext(br)}`);
    if (dev) lines.push(`- Device: ${stringifyContext(dev)}`);
    if (rt) lines.push(`- Runtime: ${stringifyContext(rt)}`);
    if (app) lines.push(`- App: ${stringifyContext(app)}`);
  }
  if (event.release && !lines.some((l) => l.startsWith('- App:'))) {
    lines.push(`- Release: ${event.release}`);
  }
  if (lines.length === 0) return null;
  return `## Context\n${lines.join('\n')}`;
}

function stringifyContext(c: Record<string, Json>): string {
  const name = (c['name'] as string | undefined) ?? '';
  const version = (c['version'] as string | undefined) ?? '';
  const extras: string[] = [];
  for (const [k, v] of Object.entries(c)) {
    if (k === 'name' || k === 'version' || k === 'type') continue;
    if (v == null) continue;
    extras.push(`${k}=${typeof v === 'object' ? JSON.stringify(v) : String(v)}`);
  }
  const head = [name, version].filter(Boolean).join(' ');
  return extras.length > 0 ? `${head} (${extras.join(', ')})` : head || JSON.stringify(c);
}

function userSection(user: Json): string | null {
  if (!user || typeof user !== 'object') return null;
  const obj = user as Record<string, Json>;
  const lines = Object.entries(obj)
    .filter(([, v]) => v != null && (typeof v !== 'object' || v !== null))
    .map(([k, v]) => `- ${k}: ${typeof v === 'object' ? JSON.stringify(v) : String(v)}`);
  if (lines.length === 0) return null;
  return `## User\n${lines.join('\n')}`;
}

function requestSection(request: Json): string | null {
  if (!request || typeof request !== 'object') return null;
  const r = request as Record<string, Json>;
  const lines: string[] = [];
  if (r['url']) lines.push(`- URL: ${r['url']}`);
  if (r['method']) lines.push(`- Method: ${r['method']}`);
  if (r['query_string']) lines.push(`- Query: ${r['query_string']}`);
  const headers = r['headers'];
  if (headers && typeof headers === 'object') {
    for (const [k, v] of Object.entries(headers)) {
      lines.push(`- ${k}: ${v}`);
    }
  }
  if (lines.length === 0) return null;
  return `## Request\n${lines.join('\n')}`;
}

function rawSection(stored: NormalizedEvent['raw']): string {
  return `## Raw payload\n\`\`\`json\n${JSON.stringify(stored, null, 2)}\n\`\`\``;
}
