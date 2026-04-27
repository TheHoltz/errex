# errex web — redesign minimalista icon-focused — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implementar o redesign minimalista do app web errex (rail vertical + topbar fina + ações icon-only com tooltip), conforme o spec `docs/superpowers/specs/2026-04-26-app-shell-icon-redesign-design.md`.

**Architecture:** Substitui o header horizontal atual (`+layout.svelte`) por (a) `<aside>` rail vertical 44px à esquerda contendo logo, project switcher (Popover), busca (CommandPalette trigger), connection status e freshness; (b) `<header>` topbar fina 34px com breadcrumb e HeaderStats. Lista e detalhe são compactados — filtros viram toggle chips de ícone, ações da issue viram icon-only com Tooltip+kbd, badges textuais viram ícones. Mantém todos os fluxos, atalhos e comportamentos existentes.

**Tech Stack:** SvelteKit (runes/Svelte 5) + shadcn-svelte + lucide-svelte + Tailwind v4. Package manager: **bun**. Sem framework de testes — verificação por `bun run check` (svelte-check) + smoke manual no browser via `bun dev`.

**Notes for the implementer:**
- Repositório **não é git** — pule qualquer passo de commit. Trabalhe direto na árvore.
- Use `bun` em todos os comandos (não `npm`/`pnpm`).
- Após cada Task: rode `bun run check` no `web/` e abra `http://localhost:5173` (deixe `bun dev` rodando em background) pra validar visualmente. Se houver `errexd` rodando, a tela popula com dados reais; se não, vai aparecer empty state — também é válido pra inspecionar layout.
- Mantenha cópia em **português** em todos os textos visíveis (já é assim hoje).
- Lucide imports: `import { IconName } from 'lucide-svelte'`.

---

## File Structure

| Arquivo | Tipo | Responsabilidade após o redesign |
|---|---|---|
| `web/src/lib/components/ui/tooltip/` | Criar (shadcn) | Primitivo de Tooltip |
| `web/src/lib/components/ui/popover/` | Criar (shadcn) | Primitivo de Popover |
| `web/src/lib/components/ui/avatar/` | Criar (shadcn) | Primitivo de Avatar |
| `web/src/lib/components/ConnectionStatus.svelte` | Modificar | Ícone Wifi/WifiOff com Tooltip |
| `web/src/lib/components/Freshness.svelte` | Modificar | Ícone RefreshCw com Tooltip |
| `web/src/lib/components/ProjectSelector.svelte` | Modificar | Trigger via Popover; conteúdo idêntico |
| `web/src/lib/components/HeaderStats.svelte` | Modificar | Fontes 12/10 pra topbar fina |
| `web/src/lib/components/IssueRow.svelte` | Modificar | Linha 36px; Flame/Avatar/ícones substituem texto e badges |
| `web/src/lib/components/IssueList.svelte` | Modificar | Filtros viram toggle chips; remove Checkbox+Label |
| `web/src/lib/components/IssueDetail.svelte` | Modificar | Header compacto; ações icon-only; section headers com ícone |
| `web/src/routes/+layout.svelte` | Modificar | Substitui header horizontal por rail vertical + topbar fina |

Sem mudanças: `+page.svelte`, `issues/[id]/+page.svelte`, `KeyboardShortcuts`, `CommandPalette`, `Toaster`, `StackTrace`, `StackFrame`, `Breadcrumbs`, `Tags`, `Sparkline`, stores, types, utilitários.

---

## Task 1: Adicionar primitivos shadcn (tooltip, popover, avatar)

**Files:**
- Create: `web/src/lib/components/ui/tooltip/`
- Create: `web/src/lib/components/ui/popover/`
- Create: `web/src/lib/components/ui/avatar/`

- [ ] **Step 1.1: Adicionar tooltip**

```bash
cd web && bunx shadcn-svelte@latest add tooltip
```

Aceite os defaults se perguntar. Esperado: cria `src/lib/components/ui/tooltip/{index.ts, tooltip-content.svelte, tooltip-trigger.svelte, tooltip.svelte}` (ou conjunto similar).

- [ ] **Step 1.2: Adicionar popover**

```bash
cd web && bunx shadcn-svelte@latest add popover
```

Esperado: cria `src/lib/components/ui/popover/...`.

- [ ] **Step 1.3: Adicionar avatar**

```bash
cd web && bunx shadcn-svelte@latest add avatar
```

Esperado: cria `src/lib/components/ui/avatar/...`.

- [ ] **Step 1.4: Verificar**

```bash
cd web && ls src/lib/components/ui/tooltip src/lib/components/ui/popover src/lib/components/ui/avatar && bun run check
```

Esperado: 3 diretórios listados, `bun run check` sai com 0 erros e 0 warnings (ou os mesmos pré-existentes).

---

## Task 2: ConnectionStatus → ícone com tooltip

**Files:**
- Modify: `web/src/lib/components/ConnectionStatus.svelte` (substitui o conteúdo inteiro)

- [ ] **Step 2.1: Substituir conteúdo do arquivo**

Substituir todo `web/src/lib/components/ConnectionStatus.svelte` por:

```svelte
<script lang="ts">
  import { Wifi, WifiOff } from 'lucide-svelte';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { connection } from '$lib/stores.svelte';
  import { cn } from '$lib/utils';

  const isConnected = $derived(connection.status === 'connected');
  const isPending = $derived(
    connection.status === 'reconnecting' || connection.status === 'connecting'
  );

  const iconClass = $derived(
    cn(
      'h-3.5 w-3.5',
      isConnected
        ? 'text-emerald-500'
        : isPending
          ? 'text-amber-500 animate-pulse'
          : 'text-destructive'
    )
  );

  const label = $derived(
    isConnected
      ? `connected${connection.serverVersion ? ` · v${connection.serverVersion}` : ''}`
      : connection.status
  );
</script>

<Tooltip.Root>
  <Tooltip.Trigger
    class="text-muted-foreground hover:text-foreground inline-flex h-6 w-6 items-center justify-center rounded-md transition-colors"
    aria-label={label}
  >
    {#if isConnected || isPending}
      <Wifi class={iconClass} />
    {:else}
      <WifiOff class={iconClass} />
    {/if}
  </Tooltip.Trigger>
  <Tooltip.Content side="right">
    {label}
  </Tooltip.Content>
</Tooltip.Root>
```

> Nota: o import `* as Tooltip from '$lib/components/ui/tooltip'` assume o padrão shadcn-svelte do `index.ts` que reexporta `Root`, `Trigger`, `Content`. Se o shadcn add tiver gerado outro shape (ex.: `TooltipRoot`, `TooltipTrigger` exports diretos), ajuste o import lendo o `index.ts` gerado e reescreva o JSX correspondente. Mesma observação vale para `Popover` e `Avatar` nas tasks seguintes.

- [ ] **Step 2.2: Verificar tipos**

```bash
cd web && bun run check
```

Esperado: 0 erros.

- [ ] **Step 2.3: Smoke visual**

Com `bun dev` rodando, abra `http://localhost:5173`. ConnectionStatus ainda está no header horizontal antigo (a Task 9 vai mover ele pro rail). Verifique:
- Aparece um ícone Wifi pequeno no canto direito do header (substituindo o badge de texto).
- Hover mostra tooltip com `connected · vX.Y.Z` (ou status atual).
- Cor do ícone: verde se conectado, âmbar pulsando se reconectando, vermelho se desconectado.

---

## Task 3: Freshness → ícone com tooltip

**Files:**
- Modify: `web/src/lib/components/Freshness.svelte` (substitui o conteúdo inteiro)

- [ ] **Step 3.1: Substituir conteúdo**

Substituir todo `web/src/lib/components/Freshness.svelte` por:

```svelte
<script lang="ts">
  import { RefreshCw } from 'lucide-svelte';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { eventStream } from '$lib/eventStream.svelte';
  import { connection } from '$lib/stores.svelte';
  import { cn } from '$lib/utils';

  const label = $derived.by(() => {
    void eventStream.tick;
    if (eventStream.lastAt == null) return 'sem eventos ainda';
    const seconds = Math.floor((Date.now() - eventStream.lastAt) / 1000);
    if (seconds < 5) return 'agora mesmo';
    if (seconds < 60) return `há ${seconds}s`;
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `há ${minutes}min`;
    const hours = Math.floor(minutes / 60);
    return `há ${hours}h`;
  });

  const stale = $derived.by(() => {
    void eventStream.tick;
    if (connection.status !== 'connected') return false;
    if (eventStream.lastAt == null) return false;
    return Date.now() - eventStream.lastAt > 120_000;
  });

  const fresh = $derived.by(() => {
    void eventStream.tick;
    if (eventStream.lastAt == null) return false;
    return Date.now() - eventStream.lastAt < 5_000;
  });
</script>

<Tooltip.Root>
  <Tooltip.Trigger
    class="text-muted-foreground hover:text-foreground inline-flex h-6 w-6 items-center justify-center rounded-md transition-colors"
    aria-label={`Último evento ${label}`}
  >
    <RefreshCw
      class={cn(
        'h-3.5 w-3.5',
        stale && 'opacity-50',
        fresh && 'animate-pulse text-foreground'
      )}
    />
  </Tooltip.Trigger>
  <Tooltip.Content side="right">
    Último evento {label}
  </Tooltip.Content>
</Tooltip.Root>
```

- [ ] **Step 3.2: Verificar tipos**

```bash
cd web && bun run check
```

Esperado: 0 erros.

- [ ] **Step 3.3: Smoke visual**

Verifique no browser: ícone RefreshCw aparece no header (no lugar do texto "Último evento ..."). Hover mostra o tooltip com a cópia completa.

---

## Task 4: ProjectSelector → trigger via Popover

**Files:**
- Modify: `web/src/lib/components/ProjectSelector.svelte` (substitui o conteúdo inteiro)

> O Popover é trigger-less aqui — quem abre é o botão `FolderKanban` no rail (Task 9). Esse componente passa a expor um `Popover.Content` que recebe o trigger via slot. Pra isso, o componente fica responsável tanto pelo trigger quanto pelo conteúdo.

- [ ] **Step 4.1: Substituir conteúdo**

Substituir todo `web/src/lib/components/ProjectSelector.svelte` por:

```svelte
<script lang="ts">
  import { ChevronsUpDown, FolderKanban } from 'lucide-svelte';
  import * as Popover from '$lib/components/ui/popover';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { cn } from '$lib/utils';
  import { projects } from '$lib/stores.svelte';
  import { connect } from '$lib/ws';

  type Props = { variant?: 'rail' | 'inline' };
  let { variant = 'rail' }: Props = $props();

  let open = $state(false);

  const items = $derived(
    projects.available.length > 0
      ? projects.available.map((p) => ({ value: p.project, label: p.project, count: p.issue_count }))
      : [{ value: projects.current, label: projects.current, count: null as number | null }]
  );

  function pick(next: string) {
    open = false;
    if (next === projects.current) return;
    connect(next);
  }
</script>

<Popover.Root bind:open>
  <Popover.Trigger>
    {#snippet child({ props })}
      {#if variant === 'rail'}
        <Tooltip.Root>
          <Tooltip.Trigger
            {...props}
            class="text-muted-foreground hover:text-foreground inline-flex h-6 w-6 items-center justify-center rounded-md transition-colors data-[state=open]:bg-accent data-[state=open]:text-foreground"
            aria-label="Trocar projeto"
          >
            <FolderKanban class="h-3.5 w-3.5" />
          </Tooltip.Trigger>
          <Tooltip.Content side="right">Projeto: {projects.current}</Tooltip.Content>
        </Tooltip.Root>
      {:else}
        <button
          {...props}
          type="button"
          class="text-muted-foreground hover:text-foreground inline-flex items-center gap-1 text-[11px] tracking-tight transition-colors"
        >
          {projects.current}
          <ChevronsUpDown class="h-3 w-3" />
        </button>
      {/if}
    {/snippet}
  </Popover.Trigger>
  <Popover.Content side={variant === 'rail' ? 'right' : 'bottom'} align="start" class="w-56 p-1">
    <ul class="flex flex-col">
      {#each items as item (item.value)}
        <li>
          <button
            type="button"
            onclick={() => pick(item.value)}
            class={cn(
              'hover:bg-accent flex w-full items-center justify-between rounded-sm px-2 py-1.5 text-[12px]',
              item.value === projects.current && 'bg-accent/60 font-medium'
            )}
          >
            <span class="truncate">{item.label}</span>
            {#if item.count != null}
              <span class="text-muted-foreground tabular-nums text-[10px]">{item.count}</span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  </Popover.Content>
</Popover.Root>
```

> Nota sobre `Popover.Trigger` + `child` snippet: bits-ui v1 (já no projeto) usa `child` snippet pra delegar o trigger pra um elemento custom. Se o Popover gerado pelo shadcn add usar API diferente, leia o `index.ts` e ajuste — o objetivo é: clicar no botão (trigger) abre o Popover com a lista.

- [ ] **Step 4.2: Verificar tipos**

```bash
cd web && bun run check
```

Esperado: 0 erros.

- [ ] **Step 4.3: Smoke visual**

Como ele ainda está no header antigo, vai aparecer no formato `inline` (a Task 9 vai usar `variant="rail"`). Verifique:
- No header aparece "default ⇕" pequeno.
- Clicar abre dropdown com a lista de projetos e contagens.
- Selecionar um projeto chama `connect(next)` (a tela recarrega dados).

---

## Task 5: HeaderStats → fontes compactas

**Files:**
- Modify: `web/src/lib/components/HeaderStats.svelte` (apenas as classes do template)

- [ ] **Step 5.1: Reduzir fontes do template**

No `web/src/lib/components/HeaderStats.svelte`, substitua o bloco `<div class="flex items-center gap-5">...</div>` (linhas 47–69) por:

```svelte
<div class="flex items-center gap-4">
  <div class="flex items-baseline gap-1.5">
    <span class="text-foreground text-[12px] font-semibold tabular-nums">{newLastHour}</span>
    <span class="text-muted-foreground text-[9px] uppercase tracking-wider">novos·1h</span>
  </div>
  <div class="bg-border h-3 w-px"></div>
  <div class="flex items-baseline gap-1.5">
    <span
      class="text-[12px] font-semibold tabular-nums {spiking > 0 ? 'text-amber-400' : 'text-foreground'}"
    >
      {spiking}
    </span>
    <span class="text-muted-foreground text-[9px] uppercase tracking-wider">spike</span>
  </div>
  <div class="bg-border h-3 w-px"></div>
  <div class="flex items-center gap-2">
    <div class="flex items-baseline gap-1.5">
      <span class="text-foreground text-[12px] font-semibold tabular-nums">{ratePerMin}</span>
      <span class="text-muted-foreground text-[9px] uppercase tracking-wider">e/min</span>
    </div>
    <Sparkline values={buckets} width={60} height={12} accent={ratePerMin > 0} />
  </div>
</div>
```

Mudanças: gap menor (5→4), fonte número 13→12, label 10→9, label de "novos · 1h" → "novos·1h", "spiking" → "spike", "events/min" → "e/min", separador `h-4`→`h-3`, sparkline `80x16`→`60x12`. Toda a `<script>` permanece como está.

- [ ] **Step 5.2: Verificar tipos**

```bash
cd web && bun run check
```

Esperado: 0 erros.

- [ ] **Step 5.3: Smoke visual**

Stats ficam visivelmente menores no header. Layout do header não quebra.

---

## Task 6: IssueRow → linha 36px com ícones

**Files:**
- Modify: `web/src/lib/components/IssueRow.svelte` (substitui o conteúdo inteiro)

- [ ] **Step 6.1: Substituir conteúdo**

Substituir todo `web/src/lib/components/IssueRow.svelte` por:

```svelte
<script lang="ts">
  import { Ban, Bell, BellOff, Check, Flame } from 'lucide-svelte';
  import { actions } from '$lib/actions.svelte';
  import { Badge } from '$lib/components/ui/badge';
  import * as Avatar from '$lib/components/ui/avatar';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { eventStream } from '$lib/eventStream.svelte';
  import type { Issue } from '$lib/types';
  import { cn, relativeTime } from '$lib/utils';
  import Sparkline from './Sparkline.svelte';

  type Props = {
    issue: Issue;
    selected?: boolean;
    onSelect?: (id: number) => void;
  };

  let { issue, selected = false, onSelect }: Props = $props();

  function levelDot(level: string | null | undefined): string {
    switch (level) {
      case 'fatal':
        return 'bg-red-500';
      case 'error':
        return 'bg-destructive';
      case 'warning':
        return 'bg-amber-500';
      case 'info':
        return 'bg-blue-400';
      case 'debug':
        return 'bg-muted-foreground';
      default:
        return 'bg-muted-foreground/60';
    }
  }

  const dotClass = $derived(levelDot(issue.level));
  const countVariant: 'destructive' | 'secondary' = $derived(
    issue.event_count >= 100 ? 'destructive' : 'secondary'
  );

  const sparkValues = $derived.by(() => {
    void eventStream.tick;
    return eventStream.buckets(issue.id, 30);
  });

  const spiking = $derived.by(() => {
    void eventStream.tick;
    return eventStream.isSpiking(issue.id);
  });

  const local = $derived(actions.get(issue));
  const isMuted = $derived(local.status === 'muted' || local.status === 'ignored');

  const railClass = $derived(
    issue.level === 'fatal'
      ? 'before:bg-red-500'
      : issue.level === 'error'
        ? 'before:bg-destructive'
        : issue.level === 'warning'
          ? 'before:bg-amber-500'
          : 'before:bg-transparent'
  );

  const initial = $derived(local.assignee ? local.assignee[0]!.toUpperCase() : '');
</script>

<button
  type="button"
  onclick={() => onSelect?.(issue.id)}
  class={cn(
    'relative flex h-9 w-full items-center gap-2.5 px-3 text-left transition-colors',
    'hover:bg-accent border-b border-border/50',
    "before:absolute before:inset-y-0 before:left-0 before:w-0.5 before:content-['']",
    railClass,
    selected && 'bg-accent/70',
    isMuted && 'opacity-60'
  )}
>
  <span class={cn('h-1.5 w-1.5 shrink-0 rounded-full', dotClass)}></span>
  <Badge variant={countVariant} class="min-w-[2.25rem] justify-center px-1 py-0 text-[10px] tabular-nums">
    {issue.event_count}
  </Badge>
  <div class="flex min-w-0 flex-1 flex-col leading-tight">
    <span class="truncate text-[12px] font-medium text-foreground">{issue.title}</span>
    {#if issue.culprit}
      <span class="truncate font-mono text-[10px] text-muted-foreground">{issue.culprit}</span>
    {/if}
  </div>

  {#if spiking}
    <Tooltip.Root>
      <Tooltip.Trigger class="text-amber-400 shrink-0">
        <Flame class="h-3 w-3" />
      </Tooltip.Trigger>
      <Tooltip.Content>Subindo nos últimos 5 min</Tooltip.Content>
    </Tooltip.Root>
  {/if}

  {#if local.status === 'resolved'}
    <Tooltip.Root>
      <Tooltip.Trigger class="text-emerald-500 shrink-0">
        <Check class="h-3 w-3" />
      </Tooltip.Trigger>
      <Tooltip.Content>Resolvida</Tooltip.Content>
    </Tooltip.Root>
  {:else if local.status === 'muted'}
    <Tooltip.Root>
      <Tooltip.Trigger class="text-muted-foreground shrink-0">
        <BellOff class="h-3 w-3" />
      </Tooltip.Trigger>
      <Tooltip.Content>Silenciada</Tooltip.Content>
    </Tooltip.Root>
  {:else if local.status === 'ignored'}
    <Tooltip.Root>
      <Tooltip.Trigger class="text-muted-foreground shrink-0">
        <Ban class="h-3 w-3" />
      </Tooltip.Trigger>
      <Tooltip.Content>Ignorada</Tooltip.Content>
    </Tooltip.Root>
  {/if}

  {#if local.assignee}
    <Tooltip.Root>
      <Tooltip.Trigger class="shrink-0">
        <Avatar.Root class="h-4 w-4 text-[9px]">
          <Avatar.Fallback class="bg-accent text-foreground">{initial}</Avatar.Fallback>
        </Avatar.Root>
      </Tooltip.Trigger>
      <Tooltip.Content>Atribuída a {local.assignee}</Tooltip.Content>
    </Tooltip.Root>
  {/if}

  <Sparkline values={sparkValues} accent={spiking} width={36} height={12} class="shrink-0" />
  <span class="shrink-0 text-[10px] text-muted-foreground tabular-nums">
    {relativeTime(issue.last_seen)}
  </span>
</button>
```

> Verificar a API real do `Sparkline`: olhe `web/src/lib/components/Sparkline.svelte` antes de mudar o uso. Se não aceitar `width`/`height` como props, omita esses atributos (mantém o tamanho default) e ajuste só com `class="h-3 w-9"` se necessário.

- [ ] **Step 6.2: Verificar tipos**

```bash
cd web && bun run check
```

Esperado: 0 erros.

- [ ] **Step 6.3: Smoke visual**

Na lista, cada linha mede ~36px de altura (medível no DevTools). Title em 12px, culprit mono em 10px. Texto "spike" sumiu — virou ícone Flame com tooltip. Badges textuais ("resolvida"/"muted"/"ignorada") sumiram — viraram ícones. Avatar redondo de 16px com inicial substitui "UserCircle2 + nome". Sparkline menor.

---

## Task 7: IssueList → filtros como toggle chips

**Files:**
- Modify: `web/src/lib/components/IssueList.svelte` (substitui o conteúdo inteiro)

- [ ] **Step 7.1: Substituir conteúdo**

Substituir todo `web/src/lib/components/IssueList.svelte` por:

```svelte
<script lang="ts">
  import { Ban, BellOff, Check, Circle, Search, ShieldCheck } from 'lucide-svelte';
  import type { LocalStatus } from '$lib/actions.svelte';
  import { actions } from '$lib/actions.svelte';
  import { Input } from '$lib/components/ui/input';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { eventStream } from '$lib/eventStream.svelte';
  import { filter, issues, load, projects, selection, visibleIssues } from '$lib/stores.svelte';
  import { cn } from '$lib/utils';
  import IssueRow from './IssueRow.svelte';

  type Props = {
    onSelect?: (id: number) => void;
    filterRef?: { current: HTMLInputElement | null };
  };
  let { onSelect, filterRef }: Props = $props();

  let inputEl = $state<HTMLInputElement | null>(null);
  $effect(() => {
    if (filterRef && inputEl) filterRef.current = inputEl;
  });

  const visible = $derived(visibleIssues());

  type StatusChip = {
    key: LocalStatus;
    label: string;
    Icon: typeof Circle;
  };

  const chips: StatusChip[] = [
    { key: 'unresolved', label: 'Não resolvidas', Icon: Circle },
    { key: 'resolved', label: 'Resolvidas', Icon: Check },
    { key: 'muted', label: 'Silenciadas', Icon: BellOff },
    { key: 'ignored', label: 'Ignoradas', Icon: Ban }
  ];

  function isChecked(s: LocalStatus): boolean {
    return filter.statuses.has(s);
  }

  function statusCount(s: LocalStatus): number {
    const all = issues.list.filter((i) => i.project === projects.current);
    return all.filter((i) => actions.get(i).status === s).length;
  }

  const allClearLabel = $derived.by(() => {
    void eventStream.tick;
    if (eventStream.lastAt == null) return 'Aguardando primeiro evento.';
    const minutes = Math.floor((Date.now() - eventStream.lastAt) / 60_000);
    if (minutes <= 0) return 'Tudo calmo · último evento agora mesmo.';
    if (minutes === 1) return 'Tudo calmo · último evento há 1 min.';
    if (minutes < 60) return `Tudo calmo · último evento há ${minutes} min.`;
    return `Tudo calmo · último evento há ${Math.floor(minutes / 60)} h.`;
  });

  const hasActiveFilter = $derived(
    filter.query.trim().length > 0 ||
      filter.statuses.size !== 1 ||
      !filter.statuses.has('unresolved')
  );
</script>

<div class="flex h-full flex-col">
  <div class="flex items-center gap-2 border-b border-border px-3 py-2">
    <div class="relative flex-1">
      <Search class="text-muted-foreground absolute left-2 top-1/2 h-3 w-3 -translate-y-1/2" />
      <Input
        bind:ref={inputEl}
        bind:value={filter.query}
        placeholder="filtrar  /"
        class="h-7 pl-6 text-[12px]"
      />
    </div>
    <div class="flex items-center gap-1">
      {#each chips as chip (chip.key)}
        {@const on = isChecked(chip.key)}
        <Tooltip.Root>
          <Tooltip.Trigger
            onclick={() => filter.toggleStatus(chip.key)}
            aria-pressed={on}
            class={cn(
              'border-border inline-flex h-6 w-6 items-center justify-center rounded-md border transition-colors',
              on ? 'bg-accent text-foreground' : 'text-muted-foreground/60 hover:text-foreground'
            )}
          >
            <chip.Icon class="h-3 w-3" />
          </Tooltip.Trigger>
          <Tooltip.Content>{chip.label} ({statusCount(chip.key)})</Tooltip.Content>
        </Tooltip.Root>
      {/each}
    </div>
  </div>

  <div class="flex-1 overflow-y-auto">
    {#if load.initialLoad}
      <ul class="flex flex-col gap-0">
        {#each Array.from({ length: 6 }) as _, i (i)}
          <li class="border-b border-border/50 px-3 py-2">
            <div class="flex items-center gap-2">
              <Skeleton class="h-1.5 w-1.5 rounded-full" />
              <Skeleton class="h-3.5 w-9" />
              <div class="flex flex-1 flex-col gap-1">
                <Skeleton class="h-2.5 w-3/4" />
                <Skeleton class="h-2 w-1/2" />
              </div>
              <Skeleton class="h-3 w-9" />
            </div>
          </li>
        {/each}
      </ul>
    {:else if visible.length === 0 && hasActiveFilter}
      <div class="text-muted-foreground flex flex-col items-center gap-1 p-6 text-center text-[11px]">
        <p>Nenhuma issue para esse filtro.</p>
        <button
          type="button"
          onclick={() => {
            filter.query = '';
            filter.statuses = new Set<LocalStatus>(['unresolved']);
          }}
          class="text-primary hover:underline text-[11px]"
        >
          Limpar filtros
        </button>
      </div>
    {:else if visible.length === 0}
      <div
        class={cn(
          'flex flex-col items-center justify-center gap-2 px-6 py-10 text-center',
          'text-muted-foreground'
        )}
      >
        <ShieldCheck class="text-emerald-500/80 h-6 w-6" />
        <p class="text-foreground text-[12px] font-medium">{allClearLabel}</p>
        <p class="text-[11px]">Sem issues abertas no projeto.</p>
      </div>
    {:else}
      {#each visible as issue (issue.id)}
        <IssueRow {issue} selected={issue.id === selection.issueId} {onSelect} />
      {/each}
    {/if}
  </div>
</div>
```

> Mudanças vs hoje: removidos `Checkbox` e `Label`; barra de filtro virou 1 linha com input + 4 chips; fonts dos empty states reduzidos pra 11–12px; ícone `ShieldCheck` reduzido pra 24px; placeholder do input mudou pra "filtrar  /".

- [ ] **Step 7.2: Verificar tipos**

```bash
cd web && bun run check
```

Esperado: 0 erros.

- [ ] **Step 7.3: Smoke visual**

- A barra de filtro agora tem 1 linha só: input à esquerda + 4 chips à direita.
- Hover em cada chip mostra tooltip "Não resolvidas (12)" etc.
- Click no chip alterna o filtro (verifique na lista).
- Estado vazio "tudo calmo": ícone menor, texto compacto.
- Estado vazio com filtro: link "Limpar filtros" funciona.

---

## Task 8: IssueDetail → header compacto e ações icon-only

**Files:**
- Modify: `web/src/lib/components/IssueDetail.svelte` (substitui o conteúdo inteiro)

- [ ] **Step 8.1: Substituir conteúdo**

Substituir todo `web/src/lib/components/IssueDetail.svelte` por:

```svelte
<script lang="ts">
  import {
    Ban,
    Bell,
    BellOff,
    Check,
    Layers,
    Link as LinkIcon,
    MousePointerClick,
    MousePointerSquare,
    RotateCcw,
    Tag,
    UserMinus,
    UserPlus
  } from 'lucide-svelte';
  import { actions } from '$lib/actions.svelte';
  import * as Avatar from '$lib/components/ui/avatar';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { Separator } from '$lib/components/ui/separator';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { selection } from '$lib/stores.svelte';
  import { toast } from '$lib/toast.svelte';
  import type { Issue } from '$lib/types';
  import { cn, relativeTime, shortFingerprint } from '$lib/utils';
  import Breadcrumbs from './Breadcrumbs.svelte';
  import StackTrace from './StackTrace.svelte';
  import Tags from './Tags.svelte';

  type Props = { issue: Issue | null };
  let { issue }: Props = $props();

  const event = $derived(selection.event);
  const local = $derived(issue ? actions.get(issue) : null);

  function levelDot(level: string | null | undefined): string {
    switch (level) {
      case 'fatal':
        return 'bg-red-500';
      case 'error':
        return 'bg-destructive';
      case 'warning':
        return 'bg-amber-500';
      case 'info':
        return 'bg-blue-400';
      default:
        return 'bg-muted-foreground/60';
    }
  }

  function onResolve() {
    if (!issue) return;
    const status = actions.get(issue).status;
    const prev = status === 'resolved' ? actions.unresolve(issue) : actions.resolve(issue);
    toast.success(status === 'resolved' ? 'Issue reaberta' : 'Issue resolvida', {
      description: issue.title,
      undo: () => actions.restore(issue, prev)
    });
  }

  function onMute() {
    if (!issue) return;
    const status = actions.get(issue).status;
    const prev = status === 'muted' ? actions.unresolve(issue) : actions.mute(issue);
    toast.success(status === 'muted' ? 'Issue reativada' : 'Issue silenciada', {
      undo: () => actions.restore(issue, prev)
    });
  }

  function onAssign() {
    if (!issue) return;
    if (local?.assignee === actions.me) {
      const prev = actions.unassign(issue);
      toast.success('Atribuição removida', { undo: () => actions.restore(issue, prev) });
    } else {
      const prev = actions.assignToMe(issue);
      toast.success(`Atribuída a ${actions.me}`, { undo: () => actions.restore(issue, prev) });
    }
  }

  function onCopyLink() {
    if (!issue) return;
    const url = `${location.origin}/issues/${issue.id}`;
    navigator.clipboard?.writeText(url).then(
      () => toast.success('Link copiado'),
      () => toast.error('Não foi possível copiar')
    );
  }

  const assigneeInitial = $derived(local?.assignee ? local.assignee[0]!.toUpperCase() : '');
</script>

{#if !issue}
  <div class="text-muted-foreground flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
    <MousePointerSquare class="h-6 w-6 opacity-60" />
    <p class="text-[12px]">Selecione uma issue para inspecionar.</p>
    <p class="text-[11px]">
      <kbd class="border-border mx-0.5 rounded border px-1 font-mono">j</kbd>/<kbd
        class="border-border mx-0.5 rounded border px-1 font-mono">k</kbd
      > pra navegar.
    </p>
  </div>
{:else}
  <div class="flex h-full flex-col">
    <header class="flex flex-col gap-2 border-b border-border px-5 py-3">
      <h1 class="text-[14px] font-semibold tracking-tight">{issue.title}</h1>

      <div class="flex flex-wrap items-center gap-1.5">
        {#if issue.level}
          <Badge variant="outline" class="gap-1 px-1.5 py-0 text-[10px]">
            <span class={cn('h-1.5 w-1.5 rounded-full', levelDot(issue.level))}></span>
            {issue.level}
          </Badge>
        {/if}
        {#if local?.status === 'resolved'}
          <Badge variant="outline" class="gap-1 px-1.5 py-0 text-[10px]">
            <Check class="text-emerald-500 h-3 w-3" /> resolvida
          </Badge>
        {:else if local?.status === 'muted'}
          <Badge variant="outline" class="gap-1 px-1.5 py-0 text-[10px]">
            <BellOff class="h-3 w-3" /> muted
          </Badge>
        {:else if local?.status === 'ignored'}
          <Badge variant="outline" class="gap-1 px-1.5 py-0 text-[10px]">
            <Ban class="h-3 w-3" /> ignorada
          </Badge>
        {/if}
        {#if local?.assignee}
          <Badge variant="outline" class="gap-1 px-1.5 py-0 text-[10px]">
            <Avatar.Root class="h-3.5 w-3.5 text-[8px]">
              <Avatar.Fallback class="bg-accent text-foreground">{assigneeInitial}</Avatar.Fallback>
            </Avatar.Root>
            {local.assignee}
          </Badge>
        {/if}
      </div>

      {#if issue.culprit}
        <p class="font-mono text-[11px] text-muted-foreground">{issue.culprit}</p>
      {/if}

      <p class="text-muted-foreground text-[10px]">
        <span class="font-mono">#{shortFingerprint(issue.fingerprint)}</span>
        · {issue.event_count} evt
        · 1º {relativeTime(issue.first_seen)}
        · últ {relativeTime(issue.last_seen)}
      </p>

      <div class="mt-1 flex items-center gap-1">
        <Tooltip.Root>
          <Tooltip.Trigger>
            {#snippet child({ props })}
              <Button {...props} variant="ghost" size="icon" class="h-7 w-7" onclick={onResolve}>
                {#if local?.status === 'resolved'}
                  <RotateCcw class="h-3.5 w-3.5" />
                {:else}
                  <Check class="h-3.5 w-3.5" />
                {/if}
              </Button>
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content>
            {local?.status === 'resolved' ? 'Reabrir' : 'Resolver'}
            <kbd class="text-muted-foreground ml-1 font-mono text-[10px]">E</kbd>
          </Tooltip.Content>
        </Tooltip.Root>

        <Tooltip.Root>
          <Tooltip.Trigger>
            {#snippet child({ props })}
              <Button {...props} variant="ghost" size="icon" class="h-7 w-7" onclick={onMute}>
                {#if local?.status === 'muted'}
                  <Bell class="h-3.5 w-3.5" />
                {:else}
                  <BellOff class="h-3.5 w-3.5" />
                {/if}
              </Button>
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content>
            {local?.status === 'muted' ? 'Reativar' : 'Silenciar'}
            <kbd class="text-muted-foreground ml-1 font-mono text-[10px]">M</kbd>
          </Tooltip.Content>
        </Tooltip.Root>

        <Tooltip.Root>
          <Tooltip.Trigger>
            {#snippet child({ props })}
              <Button {...props} variant="ghost" size="icon" class="h-7 w-7" onclick={onAssign}>
                {#if local?.assignee === actions.me}
                  <UserMinus class="h-3.5 w-3.5" />
                {:else}
                  <UserPlus class="h-3.5 w-3.5" />
                {/if}
              </Button>
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content>
            {local?.assignee === actions.me ? 'Desatribuir' : 'Atribuir a mim'}
            <kbd class="text-muted-foreground ml-1 font-mono text-[10px]">A</kbd>
          </Tooltip.Content>
        </Tooltip.Root>

        <Tooltip.Root>
          <Tooltip.Trigger>
            {#snippet child({ props })}
              <Button {...props} variant="ghost" size="icon" class="h-7 w-7" onclick={onCopyLink}>
                <LinkIcon class="h-3.5 w-3.5" />
              </Button>
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content>Copiar link</Tooltip.Content>
        </Tooltip.Root>
      </div>
    </header>

    <div class="flex-1 overflow-y-auto">
      <div class="px-5 pt-4 pb-2 flex items-center gap-2">
        <Layers class="text-muted-foreground h-3 w-3" />
        <span class="text-muted-foreground text-[10px] uppercase tracking-wider">Stack</span>
      </div>
      <StackTrace exception={event?.exception ?? null} />
      <Separator />
      <div class="px-5 pt-4 pb-2 flex items-center gap-2">
        <MousePointerClick class="text-muted-foreground h-3 w-3" />
        <span class="text-muted-foreground text-[10px] uppercase tracking-wider">Breadcrumbs</span>
      </div>
      <Breadcrumbs breadcrumbs={event?.breadcrumbs ?? []} />
      <Separator />
      <div class="px-5 pt-4 pb-2 flex items-center gap-2">
        <Tag class="text-muted-foreground h-3 w-3" />
        <span class="text-muted-foreground text-[10px] uppercase tracking-wider">Tags</span>
      </div>
      <Tags tags={event?.tags ?? {}} />
    </div>
  </div>
{/if}
```

> Sobre `Button` + `Tooltip.Trigger` com `child` snippet: bits-ui `Tooltip.Trigger` aceita `child` snippet pra delegar pro `Button` componente. Se a API do Tooltip gerada não tiver `child`, alternativa é envolver o Button num span com aria-label e pôr o tooltip ao redor desse span; ou usar `asChild`-equivalente da versão instalada. Confira o `tooltip/index.ts` antes — o JSX final deve renderizar **um** elemento clicável com o tooltip ancorado nele.

- [ ] **Step 8.2: Verificar tipos**

```bash
cd web && bun run check
```

Esperado: 0 erros.

- [ ] **Step 8.3: Smoke visual**

Selecione uma issue. Verifique:
- Header compacto: título, linha de chips pequenos com level/status/assignee, culprit mono, linha única de meta (`#... · N evt · 1º ... · últ ...`).
- 4 botões icon-only abaixo. Hover em cada um mostra tooltip com label + atalho.
- Pressione `E`/`M`/`A` — ações funcionam, ícones alternam (ex: Check ↔ RotateCcw).
- Cada seção do corpo (Stack/Breadcrumbs/Tags) tem header pequeno com ícone + label uppercase muted.
- Empty state (sem issue): ícone `MousePointerSquare` no centro + texto + dica de teclado.

---

## Task 9: +layout.svelte → rail vertical + topbar fina

**Files:**
- Modify: `web/src/routes/+layout.svelte` (substitui apenas o `<script>` body de imports + o markup; mantém os hooks `onMount`/`onDestroy` e a lógica de `paletteOpen`/`filterRef`)

- [ ] **Step 9.1: Substituir conteúdo**

Substituir todo `web/src/routes/+layout.svelte` por:

```svelte
<script lang="ts">
  import '../app.css';

  import { onDestroy, onMount, setContext } from 'svelte';
  import { AlertCircle, Search } from 'lucide-svelte';
  import { actions } from '$lib/actions.svelte';
  import CommandPalette from '$lib/components/CommandPalette.svelte';
  import ConnectionStatus from '$lib/components/ConnectionStatus.svelte';
  import Freshness from '$lib/components/Freshness.svelte';
  import HeaderStats from '$lib/components/HeaderStats.svelte';
  import KeyboardShortcuts from '$lib/components/KeyboardShortcuts.svelte';
  import ProjectSelector from '$lib/components/ProjectSelector.svelte';
  import Toaster from '$lib/components/Toaster.svelte';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { api } from '$lib/api';
  import { load, projects } from '$lib/stores.svelte';
  import { toast } from '$lib/toast.svelte';
  import { connect, disconnect } from '$lib/ws';

  let { children } = $props();

  let paletteOpen = $state(false);
  const filterRef: { current: HTMLInputElement | null } = $state({ current: null });
  setContext('filterRef', filterRef);

  onMount(async () => {
    actions.hydrate();
    try {
      const summaries = await api.projects();
      projects.available = summaries;
      const initial = summaries[0]?.project ?? projects.current ?? 'default';
      connect(initial);
    } catch (err) {
      console.warn('failed to load projects', err);
      toast.error('Não foi possível carregar projetos', {
        description: 'Verifique se o errexd está acessível.'
      });
      connect(projects.current);
    }

    setTimeout(() => {
      load.initialLoad = false;
    }, 4_000);
  });

  onDestroy(() => disconnect());
</script>

<div class="flex h-screen">
  <aside class="border-border bg-background flex w-11 shrink-0 flex-col items-center gap-3.5 border-r py-2.5">
    <a
      href="/"
      class="text-primary inline-flex h-6 w-6 items-center justify-center rounded-md"
      aria-label="errex"
      title="errex"
    >
      <AlertCircle class="h-3.5 w-3.5" />
    </a>

    <ProjectSelector variant="rail" />

    <Tooltip.Root>
      <Tooltip.Trigger
        onclick={() => (paletteOpen = true)}
        class="text-muted-foreground hover:text-foreground inline-flex h-6 w-6 items-center justify-center rounded-md transition-colors"
        aria-label="Buscar"
      >
        <Search class="h-3.5 w-3.5" />
      </Tooltip.Trigger>
      <Tooltip.Content side="right">
        Buscar <kbd class="text-muted-foreground ml-1 font-mono text-[10px]">⌘K</kbd>
      </Tooltip.Content>
    </Tooltip.Root>

    <div class="mt-auto flex flex-col items-center gap-3.5">
      <Freshness />
      <ConnectionStatus />
    </div>
  </aside>

  <div class="flex min-w-0 flex-1 flex-col">
    <header
      class="border-border bg-background flex h-[34px] shrink-0 items-center gap-3 border-b px-4"
    >
      <span class="text-muted-foreground text-[11px] tracking-tight">
        <ProjectSelector variant="inline" /> <span class="px-1 opacity-50">/</span> Issues
      </span>
      <div class="bg-border h-3 w-px"></div>
      <HeaderStats />
    </header>

    <main class="min-h-0 flex-1">
      {@render children?.()}
    </main>
  </div>
</div>

<Toaster />
<CommandPalette open={paletteOpen} onClose={() => (paletteOpen = false)} />
<KeyboardShortcuts
  onOpenPalette={() => (paletteOpen = true)}
  onFocusFilter={() => filterRef.current?.focus()}
/>
```

> O `<ProjectSelector variant="inline">` no breadcrumb e o `<ProjectSelector variant="rail">` no rail são instâncias separadas do mesmo componente — ambos compartilham o store `projects.current`, então mostram sempre o mesmo valor e qualquer mudança propaga.

- [ ] **Step 9.2: Verificar tipos**

```bash
cd web && bun run check
```

Esperado: 0 erros.

- [ ] **Step 9.3: Smoke visual completo**

Recarregue `http://localhost:5173`. Verifique:
- Rail vertical 44px à esquerda com 5 ícones (logo no topo; project switcher; busca; freshness no rodapé; connection status no rodapé).
- Topbar fina 34px com `default / Issues` à esquerda + HeaderStats compacto (3 contadores + sparkline pequeno).
- Conteúdo (lista + detalhe) ocupa o resto da tela.
- Logo: link pra `/`.
- Click no ícone FolderKanban: abre Popover à direita com lista de projetos.
- Click no ícone Search: abre CommandPalette.
- ⌘K ainda abre CommandPalette (atalho via KeyboardShortcuts).
- `/` ainda foca o input de filtro.
- `j`/`k` ainda navegam pela lista.
- `e`/`m`/`a` ainda agem na issue selecionada.

---

## Task 10: Resizable handle e polish final

**Files:**
- Modify: `web/src/routes/+page.svelte` (apenas a class do `Resizable`)

- [ ] **Step 10.1: Verificar API do Resizable**

```bash
cd web && cat src/lib/components/ui/resizable/*.svelte | head -120
```

Olhe se o `Resizable` aceita prop pra customizar o handle. Se aceitar (`handleClass` ou similar), use no Step 10.2. Caso contrário, edite o componente do handle dentro de `ui/resizable/` pra ajustar a largura/hover state.

- [ ] **Step 10.2: Aplicar handle mais sutil**

Se `Resizable` em `+page.svelte` (linha 22) aceitar uma class para o handle, passe `handleClass="w-px hover:bg-primary/40 transition-colors"`. Se não, edite o componente filho dentro de `web/src/lib/components/ui/resizable/` que renderiza o gripper, e mude as classes default pra `w-px hover:bg-primary/40 transition-colors`.

- [ ] **Step 10.3: Smoke visual**

Hover na divisória entre lista e detalhe: aparece highlight sutil em vermelho/primary. Drag ainda funciona.

---

## Task 11: Verificação final

- [ ] **Step 11.1: Type check completo**

```bash
cd web && bun run check
```

Esperado: 0 erros, 0 warnings novos.

- [ ] **Step 11.2: Build de produção**

```bash
cd web && bun run build
```

Esperado: build conclui sem erro. Tamanho de bundle similar ou menor que antes (a remoção de `Checkbox`/`Label` do header deve compensar a adição de `Tooltip`/`Popover`/`Avatar`).

- [ ] **Step 11.3: Smoke manual — golden path**

Com `bun dev` rodando + `errexd` (se possível) populando dados:

1. Carregue `/`. Esqueleto aparece, depois lista popula.
2. Clique numa issue na lista. Detalhe carrega à direita.
3. `j` e `k` navegam.
4. `/` foca o filtro. Digite algo: lista filtra. Apague: volta tudo.
5. Click em cada chip de status: filtros funcionam, tooltip mostra label + count.
6. `e` resolve a issue, toast aparece com undo. Click no undo restaura.
7. `m` silencia. Mesma coisa.
8. `a` atribui a mim. Mesma coisa.
9. Click no ícone link: link copiado, toast.
10. ⌘K abre paleta. Esc fecha.
11. Click no FolderKanban no rail: abre popover, click num projeto: troca.
12. Hover no Wifi: tooltip "connected · vX.Y.Z".
13. Hover no RefreshCw: tooltip "Último evento ...".

- [ ] **Step 11.4: Smoke manual — empty/edge states**

1. Aplique filtros que não retornam nada: estado "Nenhuma issue para esse filtro" + botão "Limpar filtros" funciona.
2. Sem issue selecionada (recarregue sem clicar): pane direito mostra `MousePointerSquare` + texto + dica `j`/`k`.
3. Pare o `errexd` (se aplicável): `WifiOff` aparece no rail em vermelho; lista mostra estado de carregamento e depois `ShieldCheck` "tudo calmo" se não houver dado em cache.

- [ ] **Step 11.5: Confirmar critérios do spec**

Cheque manualmente cada item da seção 8 do spec (`docs/superpowers/specs/2026-04-26-app-shell-icon-redesign-design.md`):
- Rail 44px sempre visível ✓
- Topbar 34px ✓
- Ações na issue: icon-only com tooltip + label + atalho ✓
- Filtros como 4 chips com count no tooltip ✓
- Linhas da lista a 36px ✓
- Atalhos `/`, `j`, `k`, `e`, `m`, `a`, `⌘k` funcionando ✓
- Estados vazios sem regressão ✓
- Build passa ✓
