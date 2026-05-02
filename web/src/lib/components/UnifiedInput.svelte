<script lang="ts">
  import { ArrowRight, Bookmark, Clock, History, Search, Star, X } from 'lucide-svelte';
  import { tick } from 'svelte';
  import { tokenize, type Token } from '$lib/queryParser';
  import { recents, savedFilters } from '$lib/savedFilters.svelte';
  import { cn } from '$lib/utils';
  import type { IssueLevel, IssueStatus, SortKey } from '$lib/types';

  // ─────────────────────────────────────────────────────────────────────
  //  UnifiedInput — single search/filter bar with inline syntax
  //  highlighting, contextual suggestions, saved-filter recall and
  //  recents. The component is presentation-only: it owns its DOM and
  //  caret state, and emits string mutations via two-way binding on
  //  `value`. Parsing into the filter store happens upstream.
  // ─────────────────────────────────────────────────────────────────────

  type Props = {
    value: string;
    /** Bindable handle to the underlying <input> element. Lets the
     *  parent wire keyboard-shortcut focus (`/` from anywhere). */
    inputEl?: HTMLInputElement | null;
    matchCount?: number; // optional live preview ("→ N issues match")
    sparkline?: number[]; // optional 60-bucket density of matches
    placeholder?: string;
    onCommit?: () => void; // called on Enter when no suggestion was accepted
    /** When true, recents are written on Enter. Set false in test runs. */
    rememberRecents?: boolean;
  };

  let {
    value = $bindable(''),
    inputEl = $bindable(null),
    matchCount,
    sparkline,
    placeholder = 'filter…',
    onCommit,
    rememberRecents = true
  }: Props = $props();

  const LEVELS: IssueLevel[] = ['fatal', 'error', 'warning', 'info', 'debug'];
  const STATUSES: IssueStatus[] = ['unresolved', 'resolved', 'muted', 'ignored'];

  // ─── Local state ─────────────────────────────────────────────────────
  let cursor = $state(value.length);
  let focused = $state(false);
  let overlay: HTMLElement | undefined = $state();

  const tokens = $derived(tokenize(value));

  // ─── Suggestion engine ───────────────────────────────────────────────
  type Suggestion = {
    kind:
      | 'level'
      | 'status'
      | 'time'
      | 'keyword'
      | 'triage'
      | 'sort'
      | 'operator'
      | 'saved'
      | 'recipe';
    label: string;
    insert: string;
    hint?: string;
  };

  const TIME_HINTS = [
    '5m',
    '15m',
    '1h',
    '24h',
    '7d',
    'last hour',
    'last night',
    'last 5 minutes',
    'last 10 minutes',
    'this morning',
    'this week',
    'today',
    'yesterday'
  ];

  const TRIAGE_HINTS: { label: string; insert: string; hint: string }[] = [
    { label: 'new', insert: 'new', hint: 'first seen in window' },
    { label: 'stale', insert: 'stale', hint: 'unresolved & gone quiet' },
    { label: 'crashing', insert: 'crashing', hint: 'level=fatal' }
  ];

  const SORT_HINTS: { label: string; insert: string; hint: string }[] = [
    { label: 'noisy', insert: 'sort:count', hint: 'most events first' },
    { label: 'most recent', insert: 'most recent', hint: 'sort by last-seen' },
    { label: 'most frequent', insert: 'most frequent', hint: 'sort by count' },
    { label: 'top 10', insert: 'top 10', hint: 'top N by count' },
    { label: 'top 100', insert: 'top 100', hint: 'top N by count' },
    { label: 'first 50', insert: 'first 50', hint: 'first N (current sort)' },
    { label: 'newest', insert: 'sort:recent', hint: 'last-seen first' },
    { label: 'oldest', insert: 'sort:stale', hint: 'last-seen last' }
  ];

  const RECIPES: { query: string; hint: string }[] = [
    { query: 'crashes overnight', hint: 'fatal level since last evening' },
    { query: 'errors in the last 10m', hint: 'errors firing right now' },
    { query: 'most recent 100 issues', hint: 'newest 100 first-seens' },
    { query: 'top 10 errors today', hint: '10 noisiest errors of the day' },
    { query: 'new last hour', hint: 'just appeared this hour' },
    { query: 'spiking unresolved', hint: 'rate at least 3× baseline' },
    { query: 'level:fatal,error noisy', hint: 'fatal+error sorted by count' },
    { query: 'tag:environment:production today', hint: 'production-only, last 24h' },
    { query: 'warnings since yesterday', hint: 'warnings in the last day' },
    { query: '"timeout*" 1h', hint: 'wildcard text + window' }
  ];

  const allSuggestions = $derived.by<Suggestion[]>(() => {
    const list: Suggestion[] = [];
    for (const l of LEVELS) list.push({ kind: 'level', label: l, insert: l });
    for (const s of STATUSES) list.push({ kind: 'status', label: s, insert: s });
    for (const t of TIME_HINTS) list.push({ kind: 'time', label: t, insert: t });
    list.push({ kind: 'keyword', label: 'spiking', insert: 'spiking' });
    for (const t of TRIAGE_HINTS)
      list.push({ kind: 'triage', label: t.label, insert: t.insert, hint: t.hint });
    for (const s of SORT_HINTS)
      list.push({ kind: 'sort', label: s.label, insert: s.insert, hint: s.hint });
    list.push({ kind: 'operator', label: 'AND', insert: 'AND', hint: 'combine clauses' });
    list.push({ kind: 'operator', label: 'OR', insert: 'OR', hint: 'either-of' });
    list.push({ kind: 'operator', label: 'NOT', insert: 'NOT', hint: 'exclude' });
    for (const sv of savedFilters.list)
      list.push({ kind: 'saved', label: sv.name, insert: `@${sv.name}`, hint: sv.query });
    return list;
  });

  // Defaults shown when the partial is empty: a curated recipe list
  // (composed queries) instead of atoms — more pedagogical, every
  // entry teaches a couple of features at once.
  const defaults = $derived.by<Suggestion[]>(() =>
    RECIPES.map((r) => ({
      kind: 'recipe' as const,
      label: r.query,
      insert: r.query,
      hint: r.hint
    }))
  );

  // The partial word the user is currently editing — the slice from
  // the last whitespace before the cursor up to the cursor.
  const partial = $derived.by(() => {
    const before = value.slice(0, cursor);
    const m = /[\S]+$/.exec(before);
    return m ? m[0] : '';
  });

  // True when the cursor sits inside a token the parser already
  // recognised (not a free-text `word`). We use this to silence fuzzy
  // completion in that case — the user finished typing a parsed
  // phrase, autocomplete shouldn't try to compete and replace part of
  // it on Enter.
  const partialIsInsideRecognizedToken = $derived.by(() => {
    if (partial.length === 0) return false;
    const partialStart = cursor - partial.length;
    for (const t of tokens) {
      if (t.kind === 'word') continue;
      if (t.span[0] <= partialStart && t.span[1] >= cursor) return true;
    }
    return false;
  });

  // Fuzzy results — suggestions whose labels actually relate to what
  // the user is currently typing. Only these are accept-via-Enter.
  const fuzzyResults = $derived.by(() => {
    const p = partial.toLowerCase();
    if (p.length === 0) return [];
    if (partialIsInsideRecognizedToken) return [];
    return allSuggestions
      .map((s) => ({ s, score: matchScore(s.label.toLowerCase(), p) }))
      .filter(({ score }) => score > 0)
      .sort((a, b) => b.score - a.score)
      .map(({ s }) => s);
  });

  const filtered = $derived(fuzzyResults.length > 0 ? fuzzyResults : defaults);
  const showingFuzzy = $derived(fuzzyResults.length > 0);
  const acceptableViaEnter = $derived(showingFuzzy || partial.length === 0);

  function matchScore(hay: string, needle: string): number {
    if (hay.startsWith(needle)) return 1000 - hay.length;
    if (hay.includes(needle)) return 500 - hay.length;
    return 0;
  }

  let highlightedIdx = $state(0);
  $effect(() => {
    void filtered;
    highlightedIdx = 0;
  });

  // Inline ghost text — the rest of the top fuzzy suggestion appended
  // after the cursor when the partial is a strict prefix.
  const ghost = $derived.by(() => {
    if (!focused || partial.length === 0) return '';
    if (!showingFuzzy) return '';
    const top = filtered[0];
    if (!top) return '';
    if (!top.label.toLowerCase().startsWith(partial.toLowerCase())) return '';
    return top.label.slice(partial.length);
  });

  // ─── Mutation helpers ────────────────────────────────────────────────
  function setCaret(pos: number) {
    cursor = pos;
    void tick().then(() => {
      inputEl?.focus();
      try {
        inputEl?.setSelectionRange(pos, pos);
      } catch {
        // defensive — number inputs throw; we use type=text.
      }
    });
  }

  function acceptSuggestion(s: Suggestion) {
    const before = value.slice(0, cursor);
    const after = value.slice(cursor);
    const partialMatch = /[\S]+$/.exec(before);
    const start = partialMatch ? before.length - partialMatch[0].length : before.length;
    const newBefore = value.slice(0, start) + s.insert;
    const sep = after.length === 0 || /^\s/.test(after) ? ' ' : '';
    value = newBefore + sep + after;
    setCaret(newBefore.length + sep.length);
  }

  function applySaved(name: string) {
    const m = savedFilters.find(name);
    if (!m) return;
    value = m.query;
    setCaret(m.query.length);
  }

  function loadRecipe(query: string) {
    value = query;
    setCaret(query.length);
  }

  function commit() {
    const trimmed = value.trim();
    if (rememberRecents && trimmed.length > 0) recents.push(trimmed);
    onCommit?.();
  }

  // Inline metacommand handlers — `save as :name` and `forget @name`
  // both fire on Enter (commit path) so the user sees the side effect
  // before the input clears.
  function handleSave(): boolean {
    const m = /save\s+as\s*:\s*([\w-]+)\s*$/i.exec(value);
    if (!m) return false;
    const cleaned = value.replace(/\s*save\s+as\s*:\s*[\w-]+\s*$/i, '').trim();
    if (cleaned.length === 0) return false;
    savedFilters.save(m[1]!, cleaned);
    value = cleaned;
    setCaret(cleaned.length);
    return true;
  }

  function handleForget(): boolean {
    const m = /^(?:forget|unsave)\s+@?([\w-]+)\s*$/i.exec(value);
    if (!m) return false;
    savedFilters.remove(m[1]!);
    value = '';
    setCaret(0);
    return true;
  }

  // ─── Keyboard ────────────────────────────────────────────────────────
  function onKeyDown(e: KeyboardEvent) {
    if (e.key === 'Tab' && ghost.length > 0) {
      e.preventDefault();
      const top = filtered[0];
      if (top) acceptSuggestion(top);
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      highlightedIdx = (highlightedIdx + 1) % Math.max(1, filtered.length);
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      highlightedIdx =
        (highlightedIdx - 1 + Math.max(1, filtered.length)) % Math.max(1, filtered.length);
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      if (handleForget()) return;
      if (handleSave()) {
        commit();
        return;
      }
      const sel = filtered[highlightedIdx];
      if (sel && acceptableViaEnter) {
        if (sel.kind === 'saved') applySaved(sel.label);
        else if (sel.kind === 'recipe') loadRecipe(sel.insert);
        else acceptSuggestion(sel);
        return;
      }
      commit();
      inputEl?.blur();
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      value = '';
      setCaret(0);
    }
  }

  function syncCursor(e: Event) {
    const ta = e.currentTarget as HTMLInputElement;
    cursor = ta.selectionStart ?? ta.value.length;
  }

  function syncScroll() {
    if (!inputEl || !overlay) return;
    overlay.scrollLeft = inputEl.scrollLeft;
  }

  // ─── Layout helpers ──────────────────────────────────────────────────
  function tokenColorClass(t: Token): string {
    switch (t.kind) {
      case 'level':
        return 'text-rose-400';
      case 'status':
        return 'text-emerald-400';
      case 'time':
        return 'text-foreground font-semibold';
      case 'keyword':
        return 'text-amber-400';
      case 'sort':
        return 'text-fuchsia-400';
      case 'limit':
        return 'text-fuchsia-300 italic';
      case 'filler':
        return 'text-muted-foreground/40 italic';
      case 'op':
        return 'text-muted-foreground italic';
      case 'neg':
        return 'text-amber-400';
      case 'text':
        return 'text-sky-400';
      case 'tag':
        return 'text-cyan-400';
      case 'url':
        return 'text-teal-400';
      case 'saved':
        return 'text-violet-400';
      case 'save':
        return 'text-violet-300 italic';
      case 'forget':
        return 'text-rose-300 italic';
      case 'word':
        return 'text-muted-foreground';
    }
  }

  function suggestionDotClass(kind: Suggestion['kind']): string {
    switch (kind) {
      case 'level':
        return 'bg-rose-400';
      case 'status':
        return 'bg-emerald-400';
      case 'time':
        return 'bg-foreground';
      case 'keyword':
      case 'triage':
        return 'bg-amber-400';
      case 'sort':
        return 'bg-fuchsia-400';
      case 'saved':
        return 'bg-violet-400';
      case 'operator':
        return 'bg-muted-foreground';
      case 'recipe':
        return 'bg-foreground/30';
    }
  }

  // Render-time syntax highlighting of the current value (mirrors the
  // input character-for-character).
  const segments = $derived.by(() => {
    const segs: { text: string; cls?: string }[] = [];
    let pos = 0;
    for (const t of tokens) {
      if (t.span[0] > pos) segs.push({ text: value.slice(pos, t.span[0]) });
      segs.push({ text: value.slice(t.span[0], t.span[1]), cls: tokenColorClass(t) });
      pos = t.span[1];
    }
    if (pos < value.length) segs.push({ text: value.slice(pos) });
    return segs;
  });

  // Same highlighter but applied to an arbitrary query (used to render
  // recipe rows in the suggestion panel — they show the canonical
  // shape with full colours so the dropdown teaches the language).
  function highlightQuery(q: string): { text: string; cls?: string }[] {
    const segs: { text: string; cls?: string }[] = [];
    const toks = tokenize(q);
    let pos = 0;
    for (const t of toks) {
      if (t.span[0] > pos) segs.push({ text: q.slice(pos, t.span[0]) });
      segs.push({ text: q.slice(t.span[0], t.span[1]), cls: tokenColorClass(t) });
      pos = t.span[1];
    }
    if (pos < q.length) segs.push({ text: q.slice(pos) });
    return segs;
  }

  // Mode badge in the trailing slot of the input.
  const modeBadge = $derived.by(() => {
    let facets = 0;
    let hasText = false;
    let hasWord = false;
    for (const t of tokens) {
      if (
        t.kind === 'level' ||
        t.kind === 'status' ||
        t.kind === 'time' ||
        t.kind === 'keyword' ||
        t.kind === 'sort' ||
        t.kind === 'limit' ||
        t.kind === 'tag' ||
        t.kind === 'url'
      ) {
        facets++;
      } else if (t.kind === 'text') {
        hasText = true;
      } else if (t.kind === 'word') {
        hasWord = true;
      }
    }
    if (facets > 0) return { label: `parsed (${facets})`, tone: 'amber' as const };
    if (hasText) return { label: 'text', tone: 'sky' as const };
    if (hasWord) return { label: 'query', tone: 'muted' as const };
    return null;
  });

  /** Public method: imperatively focus the input. */
  export function focus() {
    inputEl?.focus();
    inputEl?.select();
  }
</script>

<div class="relative w-full">
  <div class="bg-card border-border flex items-stretch gap-2 rounded-md border px-3 py-2">
    <div class="relative flex-1">
      <Search class="text-muted-foreground absolute left-2.5 top-1/2 z-10 h-4 w-4 -translate-y-1/2" />

      <!-- Highlight overlay — pointer-events: none so the input on
           top owns interaction. The overlay shares font metrics,
           padding and overflow with the input 1:1 so token highlights
           track the typed text even while horizontally scrolling. -->
      <div
        bind:this={overlay}
        aria-hidden="true"
        class="pointer-events-none absolute inset-0 overflow-hidden whitespace-pre pl-8 pr-32 font-mono text-[13px] leading-9"
      >
        {#if value.length === 0}
          <span class="text-muted-foreground/60">{placeholder}</span>
        {:else}
          {#each segments as seg, i (i)}<span class={seg.cls}>{seg.text}</span>{/each}{#if ghost.length > 0}<span
              class="text-muted-foreground/40">{ghost}</span
            >{/if}
        {/if}
      </div>

      <input
        bind:this={inputEl}
        bind:value
        type="text"
        onkeydown={onKeyDown}
        oninput={syncCursor}
        onkeyup={syncCursor}
        onclick={syncCursor}
        onselect={syncCursor}
        onscroll={syncScroll}
        onfocus={() => (focused = true)}
        onblur={() => (focused = false)}
        spellcheck="false"
        autocomplete="off"
        aria-label="Filter issues"
        class="relative block h-9 w-full bg-transparent pl-8 pr-32 font-mono text-[13px] leading-9 outline-none"
        style="color: transparent; caret-color: hsl(var(--foreground));"
      />

      <!-- Mode badge / kbd hint at the trailing edge of the input. -->
      <div class="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2">
        {#if modeBadge}
          <span
            class={cn(
              'rounded border px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-wider',
              modeBadge.tone === 'amber' && 'border-amber-500/40 bg-amber-500/10 text-amber-400',
              modeBadge.tone === 'sky' && 'border-sky-500/40 bg-sky-500/10 text-sky-400',
              modeBadge.tone === 'muted' && 'border-border text-muted-foreground'
            )}>{modeBadge.label}</span
          >
        {:else}
          <kbd
            class="border-border text-muted-foreground rounded border px-1.5 py-0.5 font-mono text-[10px]"
            >/</kbd
          >
        {/if}
      </div>
    </div>
  </div>

  {#if focused}
    <div
      class="bg-popover border-border absolute left-0 right-0 top-full z-30 mt-1 overflow-hidden rounded-md border shadow-lg"
    >
      <!-- Live match preview + sparkline (when caller passed a count). -->
      {#if matchCount != null}
        <div class="flex items-center gap-3 border-b border-border bg-muted/20 px-3 py-2">
          <ArrowRight class="text-muted-foreground h-3.5 w-3.5" />
          <span class="font-mono text-[12px]">
            <span class={cn('font-semibold', matchCount === 0 && 'text-muted-foreground')}>
              {matchCount}
            </span>
            <span class="text-muted-foreground"> issue{matchCount === 1 ? '' : 's'} match</span>
          </span>
          {#if sparkline}
            {@const max = Math.max(1, ...sparkline)}
            <div class="ml-auto flex items-end gap-px">
              {#each sparkline as count, i (i)}
                <div
                  class={cn(
                    'w-1 rounded-sm transition-all',
                    count > 0 ? 'bg-foreground/60' : 'bg-muted-foreground/15'
                  )}
                  style="height: {Math.max(2, (count / max) * 18)}px"
                ></div>
              {/each}
            </div>
          {/if}
        </div>
      {/if}

      <!-- Section header — different copy depending on whether the
           list is acceptable via Enter or just an example pile. -->
      {#if !acceptableViaEnter}
        <div
          class="text-muted-foreground/70 flex items-center justify-between border-b border-border px-3 py-1 font-mono text-[10px] uppercase tracking-wider"
        >
          <span>Examples — click to load</span>
          <span class="normal-case">
            <kbd class="bg-muted rounded px-1 text-[9px]">Enter</kbd> applies "{value.trim()}"
          </span>
        </div>
      {:else if value.length === 0}
        <div
          class="text-muted-foreground/70 border-b border-border px-3 pb-1 pt-2 font-mono text-[10px] uppercase tracking-wider"
        >
          Try
        </div>
      {/if}

      {#if filtered.length > 0}
        <ul class="py-1" role="listbox">
          {#each filtered.slice(0, 8) as s, i (s.kind + s.label)}
            {@const isHighlighted = acceptableViaEnter && i === highlightedIdx}
            {@const dotClass = suggestionDotClass(s.kind)}
            {@const showsCanonical = s.insert !== s.label && s.kind !== 'recipe'}
            <li
              role="option"
              aria-selected={isHighlighted}
              class={cn(
                'flex cursor-pointer items-baseline gap-2 px-3 py-1 text-[12px]',
                isHighlighted
                  ? 'bg-accent text-foreground'
                  : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground'
              )}
              onpointerdown={(e) => {
                e.preventDefault();
                if (s.kind === 'saved') applySaved(s.label);
                else if (s.kind === 'recipe') loadRecipe(s.insert);
                else acceptSuggestion(s);
              }}
            >
              <span
                class={cn('h-1.5 w-1.5 shrink-0 -translate-y-0.5 rounded-full', dotClass)}
                aria-hidden="true"
              ></span>
              {#if s.kind === 'recipe'}
                <span class="font-mono">
                  {#each highlightQuery(s.label) as seg, j (j)}<span class={seg.cls}
                      >{seg.text}</span
                    >{/each}
                </span>
              {:else}
                <span class="font-mono">{s.label}</span>
              {/if}
              {#if showsCanonical}
                <span class="text-muted-foreground/40 font-mono text-[11px]">→ {s.insert}</span>
              {/if}
              {#if s.hint}
                <span class="text-muted-foreground/55 truncate text-[11px]">· {s.hint}</span>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}

      {#if savedFilters.list.length > 0 && partial.length === 0}
        <div class="border-t border-border bg-muted/10">
          <div
            class="text-muted-foreground px-3 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-wider"
          >
            <Bookmark class="mr-1 inline h-2.5 w-2.5" /> Saved
          </div>
          <ul>
            {#each savedFilters.list as sv (sv.name)}
              <li class="group/saved relative">
                <button
                  type="button"
                  class="text-muted-foreground hover:bg-accent hover:text-foreground flex w-full items-center gap-3 px-3 py-1.5 pr-9 text-left text-[12px] transition-colors"
                  onclick={() => applySaved(sv.name)}
                >
                  <Star class="h-3 w-3 text-amber-400" />
                  <span class="font-mono">@{sv.name}</span>
                  <span class="text-muted-foreground/60 ml-auto truncate text-[11px]"
                    >{sv.query}</span
                  >
                </button>
                <button
                  type="button"
                  aria-label={`Remove saved filter @${sv.name}`}
                  onpointerdown={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    savedFilters.remove(sv.name);
                  }}
                  class="text-muted-foreground/40 hover:bg-foreground/10 hover:text-foreground absolute right-2 top-1/2 flex h-5 w-5 -translate-y-1/2 items-center justify-center rounded opacity-0 transition-opacity focus-visible:opacity-100 group-hover/saved:opacity-100"
                >
                  <X class="h-3 w-3" />
                </button>
              </li>
            {/each}
          </ul>
        </div>
      {/if}

      {#if recents.list.length > 0 && partial.length === 0}
        <div class="border-t border-border">
          <div
            class="text-muted-foreground px-3 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-wider"
          >
            <History class="mr-1 inline h-2.5 w-2.5" /> Recent
          </div>
          <ul>
            {#each recents.list.slice(0, 3) as r, i (i)}
              <li>
                <button
                  type="button"
                  class="text-muted-foreground hover:bg-accent hover:text-foreground flex w-full items-center gap-3 px-3 py-1.5 text-left text-[12px] transition-colors"
                  onclick={() => loadRecipe(r)}
                >
                  <Clock class="text-muted-foreground/60 h-3 w-3" />
                  <span class="font-mono">{r}</span>
                </button>
              </li>
            {/each}
          </ul>
        </div>
      {/if}

      <div
        class="text-muted-foreground/70 truncate whitespace-nowrap border-t border-border bg-muted/30 px-3 py-1.5 font-mono text-[10px]"
      >
        <kbd class="bg-muted rounded px-1 text-[9px]">Tab</kbd> accept ·
        <kbd class="bg-muted rounded px-1 text-[9px]">↑↓</kbd> navigate ·
        <kbd class="bg-muted rounded px-1 text-[9px]">Enter</kbd> apply ·
        <kbd class="bg-muted rounded px-1 text-[9px]">Esc</kbd> clear
      </div>
    </div>
  {/if}
</div>
