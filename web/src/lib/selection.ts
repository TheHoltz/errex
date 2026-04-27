// Side-effect bridge between the selection store and the event detail API.
// Components call `selectIssue(id)` instead of mutating the store directly so
// the fetch is always launched alongside the id change.

import { api } from './api';
import { normalize } from './eventDetail';
import { selection } from './stores.svelte';
import { toast } from './toast.svelte';

let inflight: AbortController | null = null;

export async function selectIssue(id: number | null): Promise<void> {
  if (selection.issueId === id) return;
  selection.issueId = id;
  selection.event = null;
  if (inflight) {
    inflight.abort();
    inflight = null;
  }
  if (id == null) return;

  selection.eventLoading = true;
  const ctrl = new AbortController();
  inflight = ctrl;
  try {
    const stored = await api.latestEvent(id);
    if (ctrl.signal.aborted || selection.issueId !== id) return;
    selection.event = stored ? normalize(stored) : null;
  } catch (err) {
    if (ctrl.signal.aborted) return;
    console.warn('selectIssue: latestEvent failed', err);
    toast.error('Failed to load event details');
  } finally {
    if (inflight === ctrl) inflight = null;
    if (selection.issueId === id) selection.eventLoading = false;
  }
}
