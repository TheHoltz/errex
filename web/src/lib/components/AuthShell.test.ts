import { render, screen } from '@testing-library/svelte';
import { describe, it, expect } from 'vitest';
import { createRawSnippet } from 'svelte';
import AuthShell from './AuthShell.svelte';

describe('AuthShell', () => {
  it('renders the title and subtitle', () => {
    render(AuthShell, {
      props: { title: 'sign in to errex', subtitle: 'self-hosted error tracking' }
    });
    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('sign in to errex');
    expect(screen.getByText('self-hosted error tracking')).toBeInTheDocument();
  });

  it('omits the subtitle paragraph when not provided', () => {
    render(AuthShell, { props: { title: 'sign in' } });
    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('sign in');
    expect(screen.queryByTestId('auth-shell-subtitle')).toBeNull();
  });

  it('marks decorative gradient layers as aria-hidden', () => {
    const { container } = render(AuthShell, { props: { title: 't' } });
    const decorative = container.querySelectorAll('[aria-hidden="true"]');
    // Two gradient layers + one noise layer = exactly 3. Tightened from
    // `toBeGreaterThanOrEqual` so a future `aria-hidden` slipping onto an
    // interactive element fails the test instead of silently passing.
    expect(decorative.length).toBe(3);
  });

  it('renders children inside the card', () => {
    // The whole point of AuthShell is to wrap a form/body — verify the slot
    // actually fires. Without this, accidentally dropping `{@render
    // children?.()}` would still pass the title/subtitle/aria-hidden tests.
    const child = createRawSnippet(() => ({
      render: () => '<button data-testid="child-content">click me</button>'
    }));
    render(AuthShell, { props: { title: 't', children: child } });
    expect(screen.getByTestId('child-content')).toHaveTextContent('click me');
  });
});
