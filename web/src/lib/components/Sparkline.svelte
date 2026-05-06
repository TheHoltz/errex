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
    /**
     * `line` (default) renders an area+line path; `bars` renders one
     * `<rect>` per bucket — used by the issue list 24h sparkline so
     * empty hours read as faint placeholders rather than a baseline.
     */
    mode?: 'line' | 'bars';
    /** Override fill color; only used in `bars` mode. */
    fill?: string;
  };

  let {
    values,
    width = 60,
    height = 14,
    class: className,
    stroke,
    accent,
    mode = 'line',
    fill
  }: Props = $props();

  // Build an SVG polyline path. Empty / all-zero series renders as a flat
  // baseline so the row height stays stable; that's preferable to omitting
  // the element and reflowing.
  const max = $derived(Math.max(1, ...values));
  // Empty if no buckets have ANY events. Drives the bars-mode dash branch
  // so issues with zero events in the window read as a single horizontal
  // mark rather than a row of low-opacity placeholders that look like
  // a loading state.
  const isEmpty = $derived(values.length === 0 || values.every((v) => v === 0));

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

  const lineStroke = $derived(
    stroke ?? (accent ? 'hsl(var(--primary))' : 'hsl(var(--muted-foreground))')
  );
  const lineFill = $derived(accent ? 'hsl(var(--primary) / 0.18)' : 'hsl(var(--muted-foreground) / 0.12)');

  // Bar layout — small inter-bar gap so adjacent buckets read distinctly
  // even when both fire at the same height.
  const barFill = $derived(fill ?? lineStroke);
  const barW = $derived(values.length > 0 ? Math.max(1, width / values.length - 1) : 0);
  const stepX = $derived(values.length > 0 ? width / values.length : width);
</script>

<svg
  viewBox={`0 0 ${width} ${height}`}
  width={width}
  height={height}
  class={cn('overflow-visible', className)}
  aria-hidden="true"
>
  {#if mode === 'bars'}
    {#if isEmpty}
      <line
        data-empty-dash
        x1={(width / 2 - 7).toFixed(2)}
        x2={(width / 2 + 7).toFixed(2)}
        y1={(height / 2).toFixed(2)}
        y2={(height / 2).toFixed(2)}
        stroke={barFill}
        stroke-width="1"
        stroke-linecap="round"
        opacity="0.4"
      />
    {:else}
      {#each values as v, i (i)}
        {@const h = v > 0 ? Math.max(1, (v / max) * height) : 1}
        <rect
          x={(i * stepX).toFixed(2)}
          y={(height - h).toFixed(2)}
          width={barW.toFixed(2)}
          height={h.toFixed(2)}
          fill={barFill}
          opacity={v > 0 ? 1 : 0}
        />
      {/each}
    {/if}
  {:else}
    <path d={fillPath} fill={lineFill} stroke="none" />
    <path d={path} fill="none" stroke={lineStroke} stroke-width="1" stroke-linejoin="round" />
  {/if}
</svg>
