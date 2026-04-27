# errex web — redesign minimalista, icon-focused

**Data:** 2026-04-26
**Escopo:** todo o app web (`web/`)
**Stack:** SvelteKit + shadcn-svelte + lucide-svelte (já instalados)
**Direção:** "Médio" — `icon rail` vertical à esquerda, topbar fina, ações icon-only com tooltip+atalho. Sem repensar fluxos; só reduzir chrome e priorizar ícones.

---

## 1. Shell

Substitui `web/src/routes/+layout.svelte` (header horizontal alto) por dois elementos:

### 1.1 Icon rail (vertical, esquerda)

- Largura **44px**, altura `h-screen`, `border-r border-border`, fundo `--background` (mesmo do app).
- Ícones lucide **14px**, container 24×24px, gap 14px entre eles, padding vertical 10px.
- Estado ativo: `bg-accent` no container + ícone full opacity. Inativo: opacity 70%.
- Tooltip shadcn (`side="right"`) em todos os itens.

| Posição | Ícone | Função | Substitui (hoje) |
|---|---|---|---|
| Topo | `AlertCircle` (cor `--primary`) | link `/`, logo errex | `<AlertCircle/> errex` no header |
| 1 | `FolderKanban` | abre `Popover` com `ProjectSelector` ancorado à direita | `ProjectSelector` no header |
| 2 | `Search` (tooltip mostra `<kbd>⌘K</kbd>`) | abre `CommandPalette` | botão "Buscar ⌘K" no header |
| Bottom | `Wifi` / `WifiOff` (animado quando offline) | `ConnectionStatus` | `ConnectionStatus` no header |
| Bottom | `RefreshCw` (pulse sutil quando dado fresco) | `Freshness`, tooltip "atualizado 4s atrás" | `Freshness` no header |

### 1.2 Topbar (horizontal, no topo do `<main>`)

- Altura **34px**, `border-b border-border`, sticky, padding horizontal 16px, gap 14px.
- **Esquerda:** breadcrumb leve `default / Issues` (font 11px muted-foreground). `default` é o projeto atual; clique abre o mesmo `Popover` do `FolderKanban` no rail.
- **Centro-esquerda:** `HeaderStats` no formato atual (3 contadores `novos·1h | spiking | events/min` + sparkline 80×16). Reduzir font do número pra 12px e label pra 10px (hoje 13/10) pra caber no topbar fino.
- **Direita:** vazio. (Freshness e Buscar migraram pro rail.)

### 1.3 Layout final

```
┌─────┬─────────────────────────────────────────────────┐
│ ⚠   │ default / Issues   12 novos · 1 spike · 47 e/m  │ topbar
│ ▦   ├─────────────────────────────────────────────────┤
│ ⌕   │                                                 │
│     │  IssueList  ┊  IssueDetail                      │ <main>
│     │             ┊                                   │
│     │  (Resizable conforme hoje)                      │
│     │                                                 │
│ ●   │                                                 │
│ ↻   │                                                 │
└─────┴─────────────────────────────────────────────────┘
```

### 1.4 Iconografia & cores globais

- Lucide-svelte exclusivamente (já no projeto).
- Tamanhos: rail 14px; botões inline 12px; section headers 12px.
- Cores: monocromático em zinc/neutral (já é a paleta `baseColor: "neutral"` do `components.json`). Acentos permitidos:
  - **Vermelho** (`--primary` / `--destructive`): logo, dot `fatal`, badge count quando ≥100.
  - **Âmbar:** dot `warning`, ícone `Flame` (spike).
  - **Azul:** dot `info`.
  - **Verde** (`emerald-500`): ícone `Check` no chip "resolvida" do detalhe e no `ShieldCheck` do empty "tudo calmo".
  - Sem outros usos de cor (sem coloração de texto, fundo, borda).
- Tooltip mostra o nome da ação + `<kbd>` do atalho quando houver.

---

## 2. Lista de issues (`IssueList.svelte` + `IssueRow.svelte`)

### 2.1 Barra de filtro (top do pane esquerdo)

- 1 linha, altura **36px**, `border-b border-border`, padding 8px/10px.
- `Input` shadcn (height 28px) com ícone `Search` interno à esquerda. Placeholder "filtrar  /".
- À direita do input, **4 toggle chips** (24×24, ícone 12px, `border border-border rounded-md`):

| Status | Ícone (lucide) | On state |
|---|---|---|
| `unresolved` | `Circle` | `bg-accent`, ícone full |
| `resolved` | `Check` | `bg-accent`, ícone full |
| `muted` | `BellOff` | `bg-accent`, ícone full |
| `ignored` | `Ban` | `bg-accent`, ícone full |

- Off: opacity 50%, sem bg.
- Tooltip = nome do status + `(N)` contagem atual.
- Remove a linha de `Checkbox + Label` atual.

### 2.2 Linha de issue (`IssueRow.svelte`)

Altura **36px** (hoje ~44px). Da esquerda pra direita:

1. Rail vertical 2px no `::before`, cor pelo level (mantém a lógica `railClass` atual).
2. Dot 6px (`bg-red-500` / `bg-destructive` / `bg-amber-500` / etc., mantém `levelDot`).
3. `Badge count` — `destructive` se ≥100, `secondary` caso contrário (mantém).
4. Bloco truncado: título 12px font-medium + culprit mono 10px muted (hoje já é assim, só reduz fontes).
5. **Spike**: ícone `Flame` 12px âmbar (substitui o texto "spike"). Tooltip "subindo nos últimos 5min".
6. **Status local** (`resolved`/`muted`/`ignored`): ícone 12px (`Check`/`BellOff`/`Ban`) no lugar dos `<Badge>` atuais. Tooltip com o label.
7. **Assignee**: `Avatar` shadcn 16px com inicial maiúscula (substitui `UserCircle2 + texto`). Tooltip com nome.
8. `Sparkline` 36×12px (hoje maior).
9. Tempo relativo 10px tabular-nums muted (mantém).

Selecionada: `bg-accent/70` (mantém). Muted: `opacity-60` (mantém).

### 2.3 Empty states

Mantém os 3 estados atuais (skeleton, "tudo calmo" com `ShieldCheck`, "nenhuma issue para esse filtro"). Ajustes:
- Ícone central: 24px (hoje 32px).
- Texto principal: 12px (hoje 13px).
- Texto secundário: 11px.

Comportamento e cópia (incluindo `allClearLabel` dinâmico) ficam idênticos.

---

## 3. Detalhe da issue (`IssueDetail.svelte`)

### 3.1 Header da issue (sticky, ~110px)

```
TypeError: cannot read 'x' of undefined
●fatal  ✓resolvida  →sam
app/checkout.ts
#a1b2c3 · 128 evt · 1º 3h · últ 2m
[✓] [🔕] [👤] [🔗]
```

- **Título:** 14px font-semibold tracking-tight (mantém).
- **Linha de chips** (substitui os `Badge` largos atuais): pequenos pills 11px monocromáticos (`Badge variant="outline"`). Cada um tem ícone/dot 8px à esquerda colorido pela semântica:
  - `●fatal` / `●error` / `●warning`: dot da cor do level.
  - `✓resolvida`: ícone `Check` verde 10px.
  - `🔕muted` / `⊘ignorada`: ícone respectivo, monocromático.
  - `→sam`: `Avatar` 14px com inicial + nome.
- **Culprit:** mantém (font mono 11px muted).
- **Meta (1 linha):** fingerprint mono · contagem · primeiro · último, separados por `·`, font 10px muted, tabular-nums no fingerprint.
- **Ações (4 botões `Button variant="ghost" size="icon"` 28×28):**

| Ícone | Ação | Estado alternativo | Atalho |
|---|---|---|---|
| `Check` | Resolver | `RotateCcw` "Reabrir" se `resolved` | E |
| `BellOff` | Silenciar | `Bell` "Reativar" se `muted` | M |
| `UserPlus` | Atribuir a mim | `UserMinus` "Desatribuir" se assignee = me | A |
| `Link` | Copiar link | — | — |

Tooltip em cada um com label + `<kbd>` do atalho. Sem texto inline.

### 3.2 Corpo (rolável)

Mantém estrutura atual: `StackTrace → Separator → Breadcrumbs → Tags`.

Cada seção ganha um header mínimo de 28px:
- `Layers` "STACK"
- `MousePointerClick` "BREADCRUMBS"
- `Tag` "TAGS"

Estilo: ícone 12px + texto 11px uppercase tracking-wider muted. `Separator` shadcn entre seções (mantém).

### 3.3 Empty state (sem issue)

```
   ┌─────┐
   │  ←  │   Selecione uma issue
   └─────┘   j/k pra navegar
```

- `MousePointerSquare` 24px muted no centro (substitui o texto puro atual).
- Texto "Selecione uma issue para inspecionar." 12px.
- Hint com `<kbd>j</kbd>`/`<kbd>k</kbd>` (mantém o conteúdo atual, só centraliza melhor verticalmente).

---

## 4. Resizable splitter

Mantém o componente `Resizable` atual em `+page.svelte`. Único ajuste:
- Handle de 1px com hover state mais visível (`hover:bg-primary/40`, `transition-colors`).

---

## 5. Componentes shadcn-svelte

Já registrados no projeto: `button`, `badge`, `input`, `checkbox`, `label`, `skeleton`, `separator`, `resizable`.

**Adicionar:**
```bash
cd web && npx shadcn-svelte@latest add tooltip popover avatar
```

Após o redesign, `checkbox` e `label` ficam órfãos (substituídos por toggle chips + tooltip). Não remover automaticamente — outros lugares podem usar. Verificar via grep antes.

---

## 6. Arquivos afetados

| Arquivo | Mudança |
|---|---|
| `web/src/routes/+layout.svelte` | Substitui `<header>` horizontal por `<aside>` rail + `<header>` topbar fina |
| `web/src/lib/components/IssueList.svelte` | Filtros viram toggle chips de ícone; remove `Checkbox`+`Label` linha |
| `web/src/lib/components/IssueRow.svelte` | Linha 36px; `Flame`/`Avatar`/ícones substituem badges e texto |
| `web/src/lib/components/IssueDetail.svelte` | Header da issue compactado; ações icon-only; section headers com ícone |
| `web/src/lib/components/HeaderStats.svelte` | Ajusta fonts (12/10) pra topbar fina |
| `web/src/lib/components/ConnectionStatus.svelte` | Vira icon-only com tooltip |
| `web/src/lib/components/Freshness.svelte` | Vira ícone `RefreshCw` com tooltip |
| `web/src/lib/components/ProjectSelector.svelte` | Conteúdo vai pra dentro de `Popover` aberto pelo rail |
| `web/src/lib/components/ui/tooltip/` | **novo** (shadcn add) |
| `web/src/lib/components/ui/popover/` | **novo** (shadcn add) |
| `web/src/lib/components/ui/avatar/` | **novo** (shadcn add) |

`+page.svelte`, `issues/[id]/+page.svelte`, `KeyboardShortcuts`, `CommandPalette`, `Toaster`, `StackTrace`, `Breadcrumbs`, `Tags`, `Sparkline`, `StackFrame` — sem mudanças.

---

## 7. Não-objetivos

- Não muda fluxos, atalhos, comandos da paleta, ou comportamento do WebSocket/store.
- Não muda copy (textos em português ficam iguais).
- Não muda paleta base (`neutral` do `components.json`).
- Não introduz novas rotas ou settings page.
- Não toca em backend (`crates/`, `errexd`).

---

## 8. Critérios de aceitação

- Rail vertical 44px sempre visível à esquerda; topbar 34px no topo.
- Todas ações na issue acessíveis por icon-only com tooltip mostrando label + atalho.
- Filtros de status visíveis como 4 chips de ícone, com count no tooltip.
- Lista renderiza linhas a 36px de altura (medível via DevTools).
- Atalhos atuais (`/`, `j`, `k`, `e`, `m`, `a`, `⌘k`) continuam funcionando.
- Sem regressão visual em estados: skeleton, "tudo calmo", "nenhuma issue para esse filtro", sem issue selecionada.
- Build passa: `cd web && bun run check && bun run build`.
