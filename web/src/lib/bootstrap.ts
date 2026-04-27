// Side effects that have to fire whenever the operator becomes signed
// in — whether that's the cookie hydrating on page load, or a fresh
// /setup or /login completing within the same tab. Keeping this out of
// `+layout.svelte`'s `onMount` matters because the layout is the
// outermost shell: it mounts once per tab. If the bootstrap lived in
// onMount and the tab opened on /setup (signed_out at hydrate time), a
// successful setup would never bring up the project list or the WS, and
// the / page would forever sit on its skeleton.

import { api } from './api';
import { load, projects } from './stores.svelte';
import { toast } from './toast.svelte';
import { connect } from './ws';

const INITIAL_LOAD_FALLBACK_MS = 4_000;

export async function bootstrapSignedIn(): Promise<void> {
  try {
    const summaries = await api.projects();
    projects.available = summaries;
    const initial = summaries[0]?.project ?? projects.current ?? 'default';
    connect(initial);
  } catch (err) {
    console.warn('failed to load projects', err);
    toast.error('Could not load projects', {
      description: 'Check that errexd is reachable.'
    });
    connect(projects.current);
  }

  // Safety net: the WS snapshot is the primary signal that flips
  // initialLoad to false (see ws.ts). This timeout covers the case where
  // the daemon never delivers a snapshot — without it, the issues view
  // sits on the skeleton forever.
  setTimeout(() => {
    load.initialLoad = false;
  }, INITIAL_LOAD_FALLBACK_MS);
}
