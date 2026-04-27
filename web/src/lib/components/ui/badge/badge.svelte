<script lang="ts" module>
  import { tv, type VariantProps } from 'tailwind-variants';

  export const badgeVariants = tv({
    base:
      'inline-flex items-center rounded-md border px-1.5 py-0.5 text-[11px] font-mono ' +
      'font-medium tracking-tight transition-colors focus:outline-none focus:ring-2 ' +
      'focus:ring-ring focus:ring-offset-1',
    variants: {
      variant: {
        default: 'border-transparent bg-primary text-primary-foreground',
        secondary: 'border-transparent bg-secondary text-secondary-foreground',
        destructive: 'border-transparent bg-destructive text-destructive-foreground',
        outline: 'text-foreground border-border',
        warning: 'border-transparent bg-amber-500/15 text-amber-400',
        info: 'border-transparent bg-blue-500/15 text-blue-400',
        success: 'border-transparent bg-emerald-500/15 text-emerald-400'
      }
    },
    defaultVariants: {
      variant: 'default'
    }
  });

  export type BadgeVariant = VariantProps<typeof badgeVariants>['variant'];
</script>

<script lang="ts">
  import { cn } from '$lib/utils';
  import type { HTMLAttributes } from 'svelte/elements';

  type Props = HTMLAttributes<HTMLDivElement> & {
    variant?: BadgeVariant;
    class?: string;
  };

  let { variant = 'default', class: className, children, ...rest }: Props = $props();
</script>

<div class={cn(badgeVariants({ variant }), className)} {...rest}>
  {@render children?.()}
</div>
