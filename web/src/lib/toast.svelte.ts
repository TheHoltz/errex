// Tiny toast queue. Auto-dismisses after `duration` ms; an optional `undo`
// action becomes a button on the toast and resolves the original action's
// promise-shape (see actions.svelte.ts callers — they keep a reference to
// the prior LocalAction and call `actions.restore()` on undo).

export type ToastVariant = 'default' | 'success' | 'warning' | 'error';

export interface ToastOptions {
  message: string;
  description?: string;
  variant?: ToastVariant;
  duration?: number;
  undo?: () => void;
}

export interface ActiveToast extends Required<Omit<ToastOptions, 'undo' | 'description'>> {
  id: number;
  description: string | null;
  undo: (() => void) | null;
}

class ToastStore {
  list = $state<ActiveToast[]>([]);
  private nextId = 1;
  private timers = new Map<number, ReturnType<typeof setTimeout>>();

  push(opts: ToastOptions): number {
    const id = this.nextId++;
    const toast: ActiveToast = {
      id,
      message: opts.message,
      description: opts.description ?? null,
      variant: opts.variant ?? 'default',
      duration: opts.duration ?? 5_000,
      undo: opts.undo ?? null
    };
    this.list = [...this.list, toast];
    if (toast.duration > 0) {
      this.timers.set(
        id,
        setTimeout(() => this.dismiss(id), toast.duration)
      );
    }
    return id;
  }

  dismiss(id: number) {
    const t = this.timers.get(id);
    if (t) {
      clearTimeout(t);
      this.timers.delete(id);
    }
    this.list = this.list.filter((x) => x.id !== id);
  }

  // Helpers — keep call sites short.
  success(message: string, opts?: Omit<ToastOptions, 'message' | 'variant'>) {
    return this.push({ ...opts, message, variant: 'success' });
  }
  error(message: string, opts?: Omit<ToastOptions, 'message' | 'variant'>) {
    return this.push({ ...opts, message, variant: 'error' });
  }
  warning(message: string, opts?: Omit<ToastOptions, 'message' | 'variant'>) {
    return this.push({ ...opts, message, variant: 'warning' });
  }
}

export const toast = new ToastStore();
