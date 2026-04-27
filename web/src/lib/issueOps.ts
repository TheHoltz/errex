// Status mutations + Undo plumbing.
//
// All status changes flow through here. The functions:
//   1. Capture the current `issue.status` (so Undo can reverse it).
//   2. Call `api.setStatus` (server is authoritative).
//   3. Show a toast with an Undo button that calls api.setStatus(prev).
//
// We intentionally do NOT do optimistic local updates: the WS broadcast
// of `IssueUpdated` lands within ~50 ms, and rolling back optimistic
// state on rare 4xx errors adds complexity that's not worth it for a
// self-host app.

import { api } from './api';
import { toast } from './toast.svelte';
import type { Issue, IssueStatus } from './types';

interface ToggleOptions {
  /** Toast headline shown on the primary transition. */
  primary: string;
  /** Toast headline shown on the inverse transition. */
  inverse: string;
}

async function setStatusWithToast(
  issue: Issue,
  next: IssueStatus,
  primary: string
): Promise<void> {
  const prev = issue.status;
  try {
    await api.setStatus(issue.id, next);
    toast.success(primary, {
      description: issue.title,
      undo: () => {
        // Fire-and-forget; if the rollback itself fails we surface a 2nd toast.
        api.setStatus(issue.id, prev).catch((err) => {
          console.warn('undo failed', err);
          toast.error('Não foi possível desfazer', { description: String(err) });
        });
      }
    });
  } catch (err) {
    toast.error('Falha ao atualizar status', { description: String(err) });
    throw err;
  }
}

/** Resolve ↔ Unresolved (a.k.a. the `e` shortcut). */
export function toggleResolve(issue: Issue, opts: Partial<ToggleOptions> = {}): Promise<void> {
  if (issue.status === 'resolved') {
    return setStatusWithToast(issue, 'unresolved', opts.inverse ?? 'Issue reaberta');
  }
  return setStatusWithToast(issue, 'resolved', opts.primary ?? 'Issue resolvida');
}

/** Mute ↔ Unresolved (`m`). */
export function toggleMute(issue: Issue, opts: Partial<ToggleOptions> = {}): Promise<void> {
  if (issue.status === 'muted') {
    return setStatusWithToast(issue, 'unresolved', opts.inverse ?? 'Issue reativada');
  }
  return setStatusWithToast(issue, 'muted', opts.primary ?? 'Issue silenciada');
}

/** Ignore ↔ Unresolved (`i`). */
export function toggleIgnore(issue: Issue): Promise<void> {
  if (issue.status === 'ignored') {
    return setStatusWithToast(issue, 'unresolved', 'Issue reativada');
  }
  return setStatusWithToast(issue, 'ignored', 'Issue ignorada');
}
