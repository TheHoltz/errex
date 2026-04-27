<script lang="ts">
  import { AlertCircle } from 'lucide-svelte';
  import type { Snippet } from 'svelte';

  type Props = {
    title: string;
    subtitle?: string;
    children?: Snippet;
  };

  let { title, subtitle, children }: Props = $props();
</script>

<div class="bg-background relative flex min-h-screen items-center justify-center overflow-hidden px-4 py-10">
  <!--
    Decorative painted backdrop. Two soft radial gradients (warm orange wash
    + cool violet wash) plus a faint inline-SVG noise overlay so the gradient
    doesn't look "video game". All three layers are aria-hidden because they
    carry no information; pointer-events: none so they never intercept clicks.
  -->
  <div
    aria-hidden="true"
    class="pointer-events-none absolute"
    style="left:-8%;top:-18%;width:70%;height:110%;
           background:radial-gradient(ellipse at 32% 50%, hsla(22,94%,53%,0.55) 0%, hsla(22,94%,53%,0.18) 32%, transparent 62%);
           filter:blur(56px);"
  ></div>
  <div
    aria-hidden="true"
    class="pointer-events-none absolute"
    style="right:-10%;bottom:-28%;width:65%;height:95%;
           background:radial-gradient(ellipse at 60% 40%, hsla(265,70%,55%,0.42) 0%, hsla(285,70%,55%,0.15) 35%, transparent 65%);
           filter:blur(64px);"
  ></div>
  <div
    aria-hidden="true"
    class="pointer-events-none absolute inset-0 mix-blend-overlay opacity-[0.06]"
    style="background-image:url(&quot;data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='180' height='180'><filter id='n'><feTurbulence type='fractalNoise' baseFrequency='0.85' numOctaves='2'/></filter><rect width='100%' height='100%' filter='url(%23n)'/></svg>&quot;);"
  ></div>

  <!--
    Glass auth-card. Not the shadcn Card primitive: the primitive's auto-
    applied gap/padding/ring fights us here, and this is a one-off pre-auth
    surface — the Card primitive stays for in-app dashboard cards.
  -->
  <div
    class="relative z-10 flex w-full max-w-[360px] flex-col gap-[18px] rounded-xl border p-[32px_30px_26px]"
    style="background:hsla(0,0%,5.5%,0.66);
           backdrop-filter:blur(22px) saturate(140%);
           -webkit-backdrop-filter:blur(22px) saturate(140%);
           border-color:hsla(0,0%,100%,0.07);
           box-shadow:0 1px 0 hsla(0,0%,100%,0.05) inset, 0 30px 80px rgba(0,0,0,0.45);"
  >
    <div class="flex flex-col gap-[3px]">
      <div
        class="mb-3 flex h-9 w-9 items-center justify-center rounded-[9px]"
        style="background:linear-gradient(140deg, hsl(22 94% 60%), hsl(36 96% 58%));
               box-shadow:0 0 0 1px hsla(22,94%,50%,0.35), 0 8px 24px hsla(22,94%,50%,0.35);"
      >
        <AlertCircle class="h-[18px] w-[18px]" style="color:hsl(22 96% 12%);" />
      </div>
      <h1 class="text-[17px] font-semibold tracking-[-0.018em]">{title}</h1>
      {#if subtitle}
        <p data-testid="auth-shell-subtitle" class="text-muted-foreground text-[11.5px]">
          {subtitle}
        </p>
      {/if}
    </div>

    {@render children?.()}
  </div>
</div>

<style>
  /* Drop the backdrop blur if the OS asks for reduced transparency.
     Falls back to the same dark Card color used elsewhere in the app. */
  @media (prefers-reduced-transparency: reduce) {
    div[style*='backdrop-filter'] {
      background: hsl(0 0% 9%) !important;
      backdrop-filter: none !important;
      -webkit-backdrop-filter: none !important;
    }
  }
</style>
