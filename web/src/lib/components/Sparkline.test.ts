import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/svelte';
import Sparkline from './Sparkline.svelte';

describe('Sparkline', () => {
  it('renders one rect per bucket in bars mode when values are non-zero', () => {
    const { container } = render(Sparkline, {
      values: [1, 2, 3, 4, 5],
      mode: 'bars',
      width: 60,
      height: 14
    });
    const rects = container.querySelectorAll('rect');
    expect(rects).toHaveLength(5);
    // No empty-state dash should be rendered alongside the bars.
    expect(container.querySelector('[data-empty-dash]')).toBeNull();
  });

  it('renders only non-zero buckets — empty buckets get opacity 0', () => {
    const { container } = render(Sparkline, {
      values: [0, 5, 0, 3, 0],
      mode: 'bars'
    });
    const rects = container.querySelectorAll('rect');
    // Each bucket still emits a <rect>, but zeros are invisible (opacity 0)
    // so they don't produce the fragmented-dots look.
    expect(rects).toHaveLength(5);
    const opacities = Array.from(rects).map((r) => r.getAttribute('opacity'));
    expect(opacities[0]).toBe('0');
    expect(opacities[2]).toBe('0');
    expect(opacities[4]).toBe('0');
    expect(opacities[1]).toBe('1');
    expect(opacities[3]).toBe('1');
  });

  it('renders an empty-state dash when sum of values is zero', () => {
    const { container } = render(Sparkline, {
      values: [0, 0, 0, 0],
      mode: 'bars'
    });
    // Dash element marks the empty state — used by IssueRow's "no events
    // in 24h" presentation. Replaces the previous fragmented-bars look.
    const dash = container.querySelector('[data-empty-dash]');
    expect(dash).not.toBeNull();
    // No bars when the dash is shown.
    expect(container.querySelectorAll('rect')).toHaveLength(0);
  });

  it('renders the dash for empty values array', () => {
    const { container } = render(Sparkline, { values: [], mode: 'bars' });
    expect(container.querySelector('[data-empty-dash]')).not.toBeNull();
  });
});
