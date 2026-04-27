<script lang="ts" module>
  import { tv, type VariantProps } from 'tailwind-variants';

  // Mirrors shadcn-svelte's Button variants but with `sm` as the default size
  // — the spec asks for compact density across the dev tool.
  export const buttonVariants = tv({
    base:
      'inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium ' +
      'ring-offset-background transition-colors focus-visible:outline-none focus-visible:ring-2 ' +
      'focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none ' +
      'disabled:opacity-50',
    variants: {
      variant: {
        default: 'bg-primary text-primary-foreground hover:bg-primary/90',
        destructive: 'bg-destructive text-destructive-foreground hover:bg-destructive/90',
        outline: 'border border-input bg-background hover:bg-accent hover:text-accent-foreground',
        secondary: 'bg-secondary text-secondary-foreground hover:bg-secondary/80',
        ghost: 'hover:bg-accent hover:text-accent-foreground',
        link: 'text-primary underline-offset-4 hover:underline'
      },
      size: {
        default: 'h-9 px-4 text-[13px]',
        sm: 'h-8 px-3 text-[12px]',
        lg: 'h-10 px-6',
        icon: 'h-8 w-8'
      }
    },
    defaultVariants: {
      variant: 'default',
      size: 'sm'
    }
  });

  export type ButtonVariant = VariantProps<typeof buttonVariants>['variant'];
  export type ButtonSize = VariantProps<typeof buttonVariants>['size'];
</script>

<script lang="ts">
  import { cn } from '$lib/utils';
  import type { HTMLButtonAttributes } from 'svelte/elements';

  type Props = HTMLButtonAttributes & {
    variant?: ButtonVariant;
    size?: ButtonSize;
    class?: string;
  };

  let {
    variant = 'default',
    size = 'sm',
    class: className,
    children,
    ...rest
  }: Props = $props();
</script>

<button class={cn(buttonVariants({ variant, size }), className)} {...rest}>
  {@render children?.()}
</button>
