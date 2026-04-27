import { render, screen } from '@testing-library/svelte';
import { describe, it, expect } from 'vitest';
import Stepper from './Stepper.svelte';

describe('Stepper', () => {
  it('renders pill 1 active and pill 2 inactive when current is 1', () => {
    const { container } = render(Stepper, {
      props: { current: 1, labels: ['verify host access', 'create account'] }
    });
    const pills = [...container.querySelectorAll('[data-stepper-pill]')];
    const [pill1, pill2] = pills;
    expect(pills).toHaveLength(2);
    expect(pill1?.getAttribute('data-stepper-pill')).toBe('active');
    expect(pill2?.getAttribute('data-stepper-pill')).toBe('inactive');
    expect(pill1).toHaveTextContent('1');
    expect(pill2).toHaveTextContent('2');
    expect(screen.getByText('verify host access')).toBeInTheDocument();
    // Inactive pills render no adjacent label — only active and done do.
    expect(screen.queryByText('create account')).toBeNull();
  });

  it('shows pill 1 done with checkmark and pill 2 active when current is 2', () => {
    const { container } = render(Stepper, {
      props: { current: 2, labels: ['verify host access', 'create account'] }
    });
    const pills = [...container.querySelectorAll('[data-stepper-pill]')];
    const [pill1, pill2] = pills;
    expect(pill1?.getAttribute('data-stepper-pill')).toBe('done');
    expect(pill2?.getAttribute('data-stepper-pill')).toBe('active');
    expect(pill1).not.toHaveTextContent('1');
    expect(pill1?.querySelector('svg')).not.toBeNull();
    expect(screen.getByText('create account')).toBeInTheDocument();
    expect(screen.getByText('verified')).toBeInTheDocument();
  });

  it('uses the doneLabel prop when provided', () => {
    render(Stepper, {
      props: { current: 2, labels: ['a', 'b'], doneLabel: 'all good' }
    });
    expect(screen.getByText('all good')).toBeInTheDocument();
    expect(screen.queryByText('verified')).toBeNull();
  });
});
