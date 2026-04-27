<script lang="ts">
  import { cn } from '$lib/utils';

  type Props = {
    values: number[];
    width?: number;
    height?: number;
    class?: string;
    /** Color overrides; default tracks `--primary`. */
    stroke?: string;
    accent?: boolean;
  };

  let { values, width = 60, height = 14, class: className, stroke, accent }: Props = $props();

  // Build an SVG polyline path. Empty / all-zero series renders as a flat
  // baseline so the row height stays stable; that's preferable to omitting
  // the element and reflowing.
  const max = $derived(Math.max(1, ...values));

  const path = $derived.by(() => {
    if (values.length === 0) return `M0 ${height} L${width} ${height}`;
    const stepX = values.length > 1 ? width / (values.length - 1) : width;
    let d = '';
    for (let i = 0; i < values.length; i++) {
      const x = i * stepX;
      const y = height - ((values[i] ?? 0) / max) * (height - 1) - 0.5;
      d += `${i === 0 ? 'M' : 'L'}${x.toFixed(1)} ${y.toFixed(1)} `;
    }
    return d.trim();
  });

  const fillPath = $derived(`${path} L${width} ${height} L0 ${height} Z`);

  const strokeColor = $derived(stroke ?? (accent ? 'hsl(var(--primary))' : 'hsl(var(--muted-foreground))'));
  const fillColor = $derived(accent ? 'hsl(var(--primary) / 0.18)' : 'hsl(var(--muted-foreground) / 0.12)');
</script>

<svg
  viewBox={`0 0 ${width} ${height}`}
  width={width}
  height={height}
  class={cn('overflow-visible', className)}
  aria-hidden="true"
>
  <path d={fillPath} fill={fillColor} stroke="none" />
  <path d={path} fill="none" stroke={strokeColor} stroke-width="1" stroke-linejoin="round" />
</svg>
