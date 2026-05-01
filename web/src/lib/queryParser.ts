// ─────────────────────────────────────────────────────────────────────
//  Filter-query parser — turns a free-form input string into a typed
//  Query. The query language is a superset of plain substring search:
//
//    • plain words        → substring match against issue titles
//    • level / status     → bare names ("fatal", "unresolved") or
//                           field-prefix forms ("level:fatal,error")
//    • time windows       → compact ("5m") or natural ("last hour",
//                           "in the last 10 minutes", "today")
//    • triage keywords    → "spiking", "new", "stale", "crashes"
//    • sort + limit       → "noisy", "most recent 100", "top 10",
//                           "sort:count", "limit:25"
//    • free text          → quoted strings ("timeout"), wildcards
//                           ("timeout*"), or `text:` prefix
//    • tags / urls        → "tag:os:windows", "url:/login"
//    • saved filters      → "@name" recall, "save as :name", "forget @name"
//    • operators          → AND / OR / NOT (uppercase), `-` for negation
//
//  The parser is defensive: anything it doesn't recognise falls
//  through as a free-text word, so users never get punished for typing
//  something the grammar doesn't yet cover.
// ─────────────────────────────────────────────────────────────────────

import type { IssueLevel, IssueStatus, SortKey } from './types';

export type FilterKeyword = 'spiking' | 'new' | 'stale' | 'crashing';

export type Token =
  | { kind: 'level';   values: IssueLevel[];   span: [number, number] }
  | { kind: 'status';  values: IssueStatus[];  span: [number, number] }
  | { kind: 'time';    ms: number; label: string; span: [number, number] }
  | { kind: 'keyword'; value: FilterKeyword; span: [number, number] }
  | { kind: 'sort';    by: SortKey; span: [number, number] }
  | { kind: 'limit';   n: number; span: [number, number] }
  | { kind: 'filler';  span: [number, number] }
  | { kind: 'op';      value: 'AND' | 'OR' | 'NOT'; span: [number, number] }
  | { kind: 'neg';     span: [number, number] }
  | { kind: 'text';    value: string; isPattern: boolean; span: [number, number] }
  | { kind: 'tag';     key: string | null; values: string[]; span: [number, number] }
  | { kind: 'url';     value: string; isPattern: boolean; span: [number, number] }
  | { kind: 'saved';   name: string; span: [number, number] }
  | { kind: 'save';    name: string; span: [number, number] }
  | { kind: 'forget';  name: string; span: [number, number] }
  | { kind: 'word';    value: string; span: [number, number] };

export type TextMatcher = { value: string; isPattern: boolean };
export type TagFilter = { key: string | null; values: string[] };

export type Query = {
  levels: Set<IssueLevel>;
  statuses: Set<IssueStatus>;
  sinceMs: number | null;
  spiking: boolean;
  newOnly: boolean;
  staleOnly: boolean;
  text: TextMatcher[];
  words: string[];
  tags: TagFilter[];
  urls: TextMatcher[];
  negLevels: Set<IssueLevel>;
  negStatuses: Set<IssueStatus>;
  negSavedNames: Set<string>;
  sort: SortKey | null;
  limit: number | null;
  saveAs: string | null;
  recallSaved: string | null;
  forgetName: string | null;
};

const LEVELS: IssueLevel[] = ['fatal', 'error', 'warning', 'info', 'debug'];
const STATUSES: IssueStatus[] = ['unresolved', 'resolved', 'muted', 'ignored'];

const HOUR = 60 * 60 * 1000;
const DAY = 24 * HOUR;

// ─── Time phrases ────────────────────────────────────────────────────
//
// Ordered longest/most-specific first. The tokenizer matches greedily
// from the cursor and shorter prefixes can otherwise eat into longer
// phrases (e.g. "last night" must beat "last").
const TIME_PATTERNS: Array<[RegExp, (m: RegExpExecArray) => { ms: number; label: string }]> = [
  [/^(?:last\s+night|overnight)\b/i, () => ({ ms: 14 * HOUR, label: 'last night' })],
  [/^this\s+morning\b/i,             () => ({ ms: 12 * HOUR, label: 'this morning' })],
  [/^(?:this|past|last)\s+week\b/i,  () => ({ ms: 7 * DAY,   label: 'this week' })],
  [/^(?:this|past|last)\s+month\b/i, () => ({ ms: 30 * DAY,  label: 'this month' })],
  [/^(?:since\s+yesterday|past\s+day)\b/i, () => ({ ms: 2 * DAY, label: 'since yesterday' })],

  // "in/over/for the last N unit"
  [
    /^(?:in|over|for)\s+the\s+(?:last|past)\s+(\d+)\s*(m|min|minute|minutes|h|hr|hour|hours|d|day|days)\b/i,
    (m) => unitParse(parseInt(m[1]!, 10), m[2]!)
  ],
  // "the last N unit"
  [
    /^the\s+(?:last|past)\s+(\d+)\s*(m|min|minute|minutes|h|hr|hour|hours|d|day|days)\b/i,
    (m) => unitParse(parseInt(m[1]!, 10), m[2]!)
  ],
  // "last N unit" / "past N unit"
  [
    /^(?:last|past)\s+(\d+)\s*(m|min|minute|minutes|h|hr|hour|hours|d|day|days)\b/i,
    (m) => unitParse(parseInt(m[1]!, 10), m[2]!)
  ],
  // "since N unit (ago)"
  [
    /^since\s+(\d+)\s*(m|min|minute|minutes|h|hr|hour|hours|d|day|days)(?:\s+ago)?\b/i,
    (m) => unitParse(parseInt(m[1]!, 10), m[2]!)
  ],
  // "N unit ago"
  [
    /^(\d+)\s*(m|min|minute|minutes|h|hr|hour|hours|d|day|days)\s+ago\b/i,
    (m) => unitParse(parseInt(m[1]!, 10), m[2]!)
  ],
  // "last hour" / "past hour" / "the last hour" / "this hour" — implicit 1
  [
    /^(?:the\s+)?(?:last|past|this)\s+(minute|min|hour|hr|day)\b/i,
    (m) => unitParse(1, m[1]!)
  ],
  // "an hour" / "a minute"
  [/^an?\s+(minute|hour|day)\b/i, (m) => unitParse(1, m[1]!)],
  // "today" / "yesterday" / "now"
  [
    /^(today|yesterday|now)\b/i,
    (m) => {
      const w = m[1]!.toLowerCase();
      if (w === 'now') return { ms: 5 * 60 * 1000, label: 'now' };
      return { ms: w === 'today' ? DAY : 2 * DAY, label: w };
    }
  ],
  // Compact: "5m" / "10min" / "1h" / "24h" / "7d"
  [
    /^(\d+)\s*(m|min|minutes|h|hr|hour|hours|d|day|days)\b/i,
    (m) => unitParse(parseInt(m[1]!, 10), m[2]!)
  ]
];

function unitParse(n: number, unit: string): { ms: number; label: string } {
  const u = unit.toLowerCase();
  if (u.startsWith('m')) return { ms: n * 60 * 1000, label: `${n}m` };
  if (u.startsWith('h') || u === 'hr') return { ms: n * 60 * 60 * 1000, label: `${n}h` };
  return { ms: n * 24 * 60 * 60 * 1000, label: `${n}d` };
}

// Walks a comma-separated list of bare values starting at `start`. Used
// for `fatal,error` (level OR) and `unresolved,resolved` (status OR)
// shorthand without the `level:` / `status:` prefix.
function parseBareCsv<T>(
  input: string,
  start: number,
  resolve: (v: string) => T | null
): { values: T[]; end: number } | null {
  const values: T[] = [];
  let i = start;
  while (i < input.length) {
    const m = /^[\w-]+/.exec(input.slice(i));
    if (!m) break;
    const v = resolve(m[0].toLowerCase());
    if (v == null) break;
    values.push(v);
    i += m[0].length;
    if (input[i] !== ',') break;
    i++;
  }
  return values.length > 0 ? { values, end: i } : null;
}

// ─── Tokenizer ───────────────────────────────────────────────────────
export function tokenize(input: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;

  while (i < input.length) {
    if (/\s/.test(input[i]!)) {
      i++;
      continue;
    }

    // "save as :name"
    const saveMatch = /^save\s+as\s*:\s*([\w-]+)/i.exec(input.slice(i));
    if (saveMatch) {
      tokens.push({ kind: 'save', name: saveMatch[1]!, span: [i, i + saveMatch[0].length] });
      i += saveMatch[0].length;
      continue;
    }

    // "forget @name" / "unsave @name"
    const forgetMatch = /^(?:forget|unsave)\s+@?([\w-]+)/i.exec(input.slice(i));
    if (forgetMatch) {
      tokens.push({ kind: 'forget', name: forgetMatch[1]!, span: [i, i + forgetMatch[0].length] });
      i += forgetMatch[0].length;
      continue;
    }

    // "top N" / "first N" — sort + limit pair, with separate spans so
    // the highlighter renders each part once.
    const topRe = /^(top|first)\s+(\d+)\b/i;
    const topM = topRe.exec(input.slice(i));
    if (topM) {
      const word = topM[1]!;
      const numStr = topM[2]!;
      const n = parseInt(numStr, 10);
      const numStart = i + topM[0].lastIndexOf(numStr);
      const numEnd = numStart + numStr.length;
      if (word.toLowerCase() === 'top') {
        tokens.push({ kind: 'sort', by: 'count', span: [i, i + word.length] });
      }
      tokens.push({ kind: 'limit', n, span: [numStart, numEnd] });
      i += topM[0].length;
      continue;
    }

    // "most recent N" / "most frequent N" / "most recent" — sort axis
    // with optional limit, again with non-overlapping spans.
    const mostRe =
      /^most\s+(recent|frequent|spiking|noisy|new|old|stale)(?:\s+(\d+))?\b/i;
    const mostM = mostRe.exec(input.slice(i));
    if (mostM) {
      const word = mostM[1]!.toLowerCase();
      const numStr = mostM[2];
      const sortBy: SortKey =
        word === 'recent' || word === 'new'
          ? 'recent'
          : word === 'old' || word === 'stale'
            ? 'stale'
            : 'count';
      if (numStr) {
        const numStart = i + mostM[0].lastIndexOf(numStr);
        const sortEnd = i + mostM[0].slice(0, numStart - i).trimEnd().length;
        tokens.push({ kind: 'sort', by: sortBy, span: [i, sortEnd] });
        tokens.push({ kind: 'limit', n: parseInt(numStr, 10), span: [numStart, numStart + numStr.length] });
      } else {
        tokens.push({ kind: 'sort', by: sortBy, span: [i, i + mostM[0].length] });
      }
      i += mostM[0].length;
      continue;
    }

    // Bare "limit:N" / "limit N"
    const limitRe = /^limit\s*:?\s*(\d+)\b/i;
    const limM = limitRe.exec(input.slice(i));
    if (limM) {
      tokens.push({ kind: 'limit', n: parseInt(limM[1]!, 10), span: [i, i + limM[0].length] });
      i += limM[0].length;
      continue;
    }

    // Time phrases — try the longest patterns first.
    let timeMatched = false;
    for (const [re, build] of TIME_PATTERNS) {
      const m = re.exec(input.slice(i));
      if (m) {
        const { ms, label } = build(m);
        tokens.push({ kind: 'time', ms, label, span: [i, i + m[0].length] });
        i += m[0].length;
        timeMatched = true;
        break;
      }
    }
    if (timeMatched) continue;

    // Operators — uppercase only, to avoid eating natural-language "and".
    const opM = /^(AND|OR|NOT)\b/.exec(input.slice(i));
    if (opM) {
      tokens.push({ kind: 'op', value: opM[1] as 'AND' | 'OR' | 'NOT', span: [i, i + opM[0].length] });
      i += opM[0].length;
      continue;
    }

    // Negation prefix `-foo`
    if (input[i] === '-' && i + 1 < input.length && /\S/.test(input[i + 1]!)) {
      tokens.push({ kind: 'neg', span: [i, i + 1] });
      i++;
      continue;
    }

    // Quoted string → free-text. Wildcards inside quotes are honoured.
    if (input[i] === '"') {
      const end = input.indexOf('"', i + 1);
      const raw = end === -1 ? input.slice(i + 1) : input.slice(i + 1, end);
      const closeAt = end === -1 ? input.length : end + 1;
      tokens.push({
        kind: 'text',
        value: raw.replace(/\*/g, ''),
        isPattern: raw.includes('*'),
        span: [i, closeAt]
      });
      i = closeAt;
      continue;
    }

    // @name → saved-filter recall
    if (input[i] === '@') {
      const m = /^@([\w-]+)/.exec(input.slice(i));
      if (m) {
        tokens.push({ kind: 'saved', name: m[1]!, span: [i, i + m[0].length] });
        i += m[0].length;
        continue;
      }
    }

    // tag:KEY:VAL[,VAL] / tag:VALUE (loose)
    const tagRe = /^tag\s*:\s*([\w-]+)(?:\s*:\s*([\w*-]+(?:,[\w*-]+)*))?/i;
    const tagM = tagRe.exec(input.slice(i));
    if (tagM) {
      const span: [number, number] = [i, i + tagM[0].length];
      if (tagM[2]) {
        const vals = tagM[2].split(',').map((v) => v.trim()).filter(Boolean);
        tokens.push({ kind: 'tag', key: tagM[1]!.toLowerCase(), values: vals, span });
      } else {
        tokens.push({ kind: 'tag', key: null, values: [tagM[1]!.toLowerCase()], span });
      }
      i += tagM[0].length;
      continue;
    }

    // url:VALUE / url:"..."
    const urlRe = /^url\s*:\s*("[^"]*"|\S+)/i;
    const urlM = urlRe.exec(input.slice(i));
    if (urlM) {
      const span: [number, number] = [i, i + urlM[0].length];
      let raw = urlM[1]!;
      const quoted = raw.startsWith('"') && raw.endsWith('"');
      if (quoted) raw = raw.slice(1, -1);
      tokens.push({
        kind: 'url',
        value: raw.replace(/\*/g, ''),
        isPattern: raw.includes('*'),
        span
      });
      i += urlM[0].length;
      continue;
    }

    // Field-prefix syntax: level: / status: / text: / is: / has: / sort:
    const fieldRe = /^(level|status|text|is|has|sort)\s*:\s*([\w*-]+(?:,[\w*-]+)*)/i;
    const fm = fieldRe.exec(input.slice(i));
    if (fm) {
      const field = fm[1]!.toLowerCase();
      const rawValues = fm[2]!.split(',').map((v) => v.trim()).filter(Boolean);
      const span: [number, number] = [i, i + fm[0].length];
      if (field === 'level') {
        const vals = rawValues
          .map((v) => LEVELS.find((l) => v.toLowerCase() === l || v.toLowerCase() === `${l}s`))
          .filter((v): v is IssueLevel => v != null);
        if (vals.length > 0) tokens.push({ kind: 'level', values: vals, span });
      } else if (field === 'status') {
        const vals = rawValues
          .map((v) => STATUSES.find((s) => v.toLowerCase() === s))
          .filter((v): v is IssueStatus => v != null);
        if (vals.length > 0) tokens.push({ kind: 'status', values: vals, span });
      } else if (field === 'text') {
        for (const v of rawValues) {
          tokens.push({ kind: 'text', value: v.replace(/\*/g, ''), isPattern: v.includes('*'), span });
        }
      } else if (field === 'is' || field === 'has') {
        for (const v of rawValues) {
          const vl = v.toLowerCase();
          const lv = LEVELS.find((l) => vl === l);
          const st = STATUSES.find((s) => vl === s);
          const kw = (['spiking', 'new', 'stale', 'crashing'] as FilterKeyword[]).find((k) => vl === k);
          if (lv) tokens.push({ kind: 'level', values: [lv], span });
          else if (st) tokens.push({ kind: 'status', values: [st], span });
          else if (kw) tokens.push({ kind: 'keyword', value: kw, span });
        }
      } else if (field === 'sort') {
        const sortMap: Record<string, SortKey> = {
          count: 'count', frequent: 'count', noisy: 'count', top: 'count', loudest: 'count',
          recent: 'recent', newest: 'recent', latest: 'recent',
          stale: 'stale', oldest: 'stale'
        };
        const by = sortMap[rawValues[0]?.toLowerCase() ?? ''];
        if (by) tokens.push({ kind: 'sort', by, span });
      }
      i += fm[0].length;
      continue;
    }

    // Bare word
    const wm = /^[\w*-]+/.exec(input.slice(i));
    if (!wm) {
      i++;
      continue;
    }
    const word = wm[0];
    const lower = word.toLowerCase();

    // Bare comma-as-OR for levels / statuses
    const csvLevels = parseBareCsv(input, i, (v) =>
      LEVELS.find((l) => v === l || v === `${l}s`) ?? null
    );
    if (csvLevels && csvLevels.values.length > 1) {
      tokens.push({ kind: 'level', values: csvLevels.values, span: [i, csvLevels.end] });
      i = csvLevels.end;
      continue;
    }
    const csvStatuses = parseBareCsv(input, i, (v) => STATUSES.find((s) => v === s) ?? null);
    if (csvStatuses && csvStatuses.values.length > 1) {
      tokens.push({ kind: 'status', values: csvStatuses.values, span: [i, csvStatuses.end] });
      i = csvStatuses.end;
      continue;
    }

    // Single-word level / status (with plurals on levels)
    const lvlMatch = LEVELS.find((l) => lower === l || lower === `${l}s`);
    if (lvlMatch) {
      tokens.push({ kind: 'level', values: [lvlMatch], span: [i, i + word.length] });
      i += word.length;
      continue;
    }
    const stMatch = STATUSES.find((s) => lower === s);
    if (stMatch) {
      tokens.push({ kind: 'status', values: [stMatch], span: [i, i + word.length] });
      i += word.length;
      continue;
    }

    // crashes / crashing → alias for level:fatal
    if (lower === 'crashes' || lower === 'crashing') {
      tokens.push({ kind: 'level', values: ['fatal'], span: [i, i + word.length] });
      i += word.length;
      continue;
    }

    // Triage / spiking keywords
    const kwMap: Record<string, FilterKeyword> = {
      spiking: 'spiking',
      new: 'new', fresh: 'new',
      stale: 'stale', old: 'stale', zombie: 'stale'
    };
    const kw = kwMap[lower];
    if (kw) {
      tokens.push({ kind: 'keyword', value: kw, span: [i, i + word.length] });
      i += word.length;
      continue;
    }

    // Sort aliases as bare words
    const sortAliases: Record<string, SortKey> = {
      noisy: 'count', top: 'count', loudest: 'count', frequent: 'count',
      newest: 'recent', latest: 'recent'
    };
    const sortAlias = sortAliases[lower];
    if (sortAlias) {
      tokens.push({ kind: 'sort', by: sortAlias, span: [i, i + word.length] });
      i += word.length;
      continue;
    }

    // Filler words — silently dropped on evaluate
    const FILLERS = new Set([
      'issues', 'issue', 'results', 'result', 'events', 'event', 'items', 'item', 'rows'
    ]);
    if (FILLERS.has(lower)) {
      tokens.push({ kind: 'filler', span: [i, i + word.length] });
      i += word.length;
      continue;
    }

    // A bare integer that immediately follows a sort token reads as a limit.
    if (/^\d+$/.test(word)) {
      const prev = tokens[tokens.length - 1];
      if (prev && prev.kind === 'sort') {
        tokens.push({ kind: 'limit', n: parseInt(word, 10), span: [i, i + word.length] });
        i += word.length;
        continue;
      }
    }

    // Wildcard text outside of quotes
    if (word.includes('*')) {
      tokens.push({
        kind: 'text',
        value: word.replace(/\*/g, ''),
        isPattern: true,
        span: [i, i + word.length]
      });
      i += word.length;
      continue;
    }

    // Numeric plural strip: "500s" → substring "500"
    if (/^\d+s$/.test(word)) {
      tokens.push({ kind: 'word', value: word.slice(0, -1), span: [i, i + word.length] });
      i += word.length;
      continue;
    }

    // Unrecognised — falls through as plain free-text
    tokens.push({ kind: 'word', value: word, span: [i, i + word.length] });
    i += word.length;
  }

  return tokens;
}

// ─── buildQuery ──────────────────────────────────────────────────────
export function buildQuery(tokens: Token[]): Query {
  const q: Query = {
    levels: new Set(),
    statuses: new Set(),
    sinceMs: null,
    spiking: false,
    newOnly: false,
    staleOnly: false,
    text: [],
    words: [],
    tags: [],
    urls: [],
    negLevels: new Set(),
    negStatuses: new Set(),
    negSavedNames: new Set(),
    sort: null,
    limit: null,
    saveAs: null,
    recallSaved: null,
    forgetName: null
  };
  for (let k = 0; k < tokens.length; k++) {
    const t = tokens[k]!;
    const prev = tokens[k - 1];
    const negated = prev?.kind === 'neg';
    switch (t.kind) {
      case 'level':
        for (const v of t.values) {
          if (negated) q.negLevels.add(v);
          else q.levels.add(v);
        }
        break;
      case 'status':
        for (const v of t.values) {
          if (negated) q.negStatuses.add(v);
          else q.statuses.add(v);
        }
        break;
      case 'time':    q.sinceMs = t.ms; break;
      case 'keyword':
        if (t.value === 'spiking')  q.spiking = true;
        if (t.value === 'new')      q.newOnly = true;
        if (t.value === 'stale')    q.staleOnly = true;
        break;
      case 'sort':    q.sort = t.by; break;
      case 'limit':   q.limit = t.n; break;
      case 'text':    q.text.push({ value: t.value, isPattern: t.isPattern }); break;
      case 'word':    q.words.push(t.value); break;
      case 'tag':     q.tags.push({ key: t.key, values: t.values.map((v) => v.toLowerCase()) }); break;
      case 'url':     q.urls.push({ value: t.value, isPattern: t.isPattern }); break;
      case 'save':    q.saveAs = t.name; break;
      case 'saved':
        if (negated) q.negSavedNames.add(t.name);
        else q.recallSaved = t.name;
        break;
      case 'forget':  q.forgetName = t.name; break;
      case 'op':
      case 'neg':
      case 'filler':
        break;
    }
  }
  return q;
}

// ─── Parse helper ────────────────────────────────────────────────────
//
// Convenience that runs both passes for callers that don't care about
// the intermediate token list.
export function parseQuery(input: string): { tokens: Token[]; query: Query } {
  const tokens = tokenize(input);
  return { tokens, query: buildQuery(tokens) };
}
