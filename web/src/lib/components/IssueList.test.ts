import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import { filter, issues, load, projects } from '$lib/stores.svelte';
import type { IssueLevel, IssueStatus } from '$lib/types';
import IssueListWrapper from './IssueListWrapper.svelte';

// IssueList.svelte requires a Tooltip.Provider ancestor (wired in the app
// layout). IssueListWrapper provides that context so the component can be
// exercised in the jsdom environment without the full SvelteKit layout.

beforeEach(() => {
  // Reset the URL so the onMount in IssueList.svelte does not re-hydrate
  // stale filter params written by a previous test's $effect.
  history.replaceState(null, '', '/');
  issues.reset([]);
  filter.query = '';
  filter.statuses = new Set<IssueStatus>(['unresolved']);
  filter.levels = new Set<IssueLevel>();
  filter.sinceMs = null;
  filter.spikingOnly = false;
  filter.sort = 'recent';
  projects.current = 'p';
  load.initialLoad = false;
});

afterEach(() => {
  issues.reset([]);
});

describe('IssueList regex tag', () => {
  it('shows the / kbd hint when query is empty', () => {
    render(IssueListWrapper);
    expect(screen.getByText('/')).toBeInTheDocument();
    expect(screen.queryByTestId('query-mode-tag')).toBeNull();
  });

  it('shows an amber "regex" tag when query starts with a valid /pattern', async () => {
    render(IssueListWrapper);
    flushSync(() => {
      filter.query = '/Error.*/';
    });
    const tag = await screen.findByTestId('query-mode-tag');
    expect(tag).toHaveTextContent('regex');
    expect(tag.className).toMatch(/text-amber-500/);
  });

  it('shows a destructive red "regex" tag when query is invalid', async () => {
    render(IssueListWrapper);
    flushSync(() => {
      filter.query = '/Error[';
    });
    const tag = await screen.findByTestId('query-mode-tag');
    expect(tag).toHaveTextContent('regex');
    expect(tag.className).toMatch(/text-destructive/);
  });

  it('hides the kbd hint and tag when in substring mode with text', async () => {
    render(IssueListWrapper);
    flushSync(() => {
      filter.query = 'auth';
    });
    expect(screen.queryByTestId('query-mode-tag')).toBeNull();
    // The clear button takes the right slot at this point; the kbd is gone too.
    expect(screen.queryByText('/')).toBeNull();
  });
});

describe('IssueList sort menu', () => {
  it('renders the sort button with no label (icon-only)', () => {
    render(IssueListWrapper);
    const btn = screen.getByRole('button', { name: /sort/i });
    expect(btn.textContent?.trim()).toBe('');
  });

  // The popover-driven click path can't be exercised in jsdom: bits-ui's
  // floating-ui positioning needs a real layout engine, so Popover.Content
  // never mounts and the menuitems aren't queryable. The active-class test
  // below covers the component's reaction to sort changes; the click → store
  // binding is verified manually in the smoke check (scripts/dev.sh).
  it('sort button gets an active ring class when sort is non-default', () => {
    render(IssueListWrapper);
    const btn = screen.getByRole('button', { name: /sort/i });
    expect(btn.className).not.toMatch(/bg-foreground\/10/);

    // flushSync forces Svelte to apply the reactive class update synchronously
    // so we can assert the new className without awaiting a tick.
    flushSync(() => {
      filter.sort = 'count';
    });
    expect(btn.className).toMatch(/bg-foreground\/10/);
    expect(btn.className).toMatch(/ring-1/);
  });
});
