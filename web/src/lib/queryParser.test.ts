import { describe, expect, it } from 'vitest';
import { parseQuery, tokenize } from './queryParser';

// Convenience — most tests only care about the resulting Query, not the
// intermediate token list. These helpers make the assertions terse.
function q(input: string) {
  return parseQuery(input).query;
}

describe('queryParser — levels', () => {
  it('parses a bare level', () => {
    expect([...q('fatal').levels]).toEqual(['fatal']);
  });

  it('parses a level plural', () => {
    expect([...q('errors').levels]).toEqual(['error']);
  });

  it('parses comma-list as OR within kind', () => {
    expect([...q('fatal,error').levels].sort()).toEqual(['error', 'fatal']);
  });

  it('parses field-prefix form', () => {
    expect([...q('level:fatal,error').levels].sort()).toEqual(['error', 'fatal']);
  });

  it('crashes / crashing alias to fatal', () => {
    expect([...q('crashes').levels]).toEqual(['fatal']);
    expect([...q('crashing').levels]).toEqual(['fatal']);
  });

  it('separates two non-comma tokens as a union', () => {
    expect([...q('fatal warning').levels].sort()).toEqual(['fatal', 'warning']);
  });

  it('negation puts level into negLevels', () => {
    const r = q('-debug');
    expect([...r.negLevels]).toEqual(['debug']);
    expect(r.levels.size).toBe(0);
  });
});

describe('queryParser — statuses', () => {
  it('parses bare status', () => {
    expect([...q('unresolved').statuses]).toEqual(['unresolved']);
  });

  it('comma-OR for statuses', () => {
    expect([...q('unresolved,muted').statuses].sort()).toEqual(['muted', 'unresolved']);
  });

  it('field-prefix form', () => {
    expect([...q('status:resolved').statuses]).toEqual(['resolved']);
  });
});

describe('queryParser — time windows', () => {
  it('compact 5m', () => {
    expect(q('5m').sinceMs).toBe(5 * 60 * 1000);
  });

  it('compact 1h', () => {
    expect(q('1h').sinceMs).toBe(60 * 60 * 1000);
  });

  it('compact 7d', () => {
    expect(q('7d').sinceMs).toBe(7 * 24 * 60 * 60 * 1000);
  });

  it('"in the last 10 minutes"', () => {
    expect(q('in the last 10 minutes').sinceMs).toBe(10 * 60 * 1000);
  });

  it('"the last 5m"', () => {
    expect(q('the last 5m').sinceMs).toBe(5 * 60 * 1000);
  });

  it('"last hour" (implicit 1)', () => {
    expect(q('last hour').sinceMs).toBe(60 * 60 * 1000);
  });

  it('"past hour" / "this hour"', () => {
    expect(q('past hour').sinceMs).toBe(60 * 60 * 1000);
    expect(q('this hour').sinceMs).toBe(60 * 60 * 1000);
  });

  it('"an hour" / "a minute"', () => {
    expect(q('an hour').sinceMs).toBe(60 * 60 * 1000);
    expect(q('a minute').sinceMs).toBe(60 * 1000);
  });

  it('"5m ago"', () => {
    expect(q('5m ago').sinceMs).toBe(5 * 60 * 1000);
  });

  it('"since 1h ago"', () => {
    expect(q('since 1h ago').sinceMs).toBe(60 * 60 * 1000);
  });

  it('"today" / "yesterday"', () => {
    expect(q('today').sinceMs).toBe(24 * 60 * 60 * 1000);
    expect(q('yesterday').sinceMs).toBe(2 * 24 * 60 * 60 * 1000);
  });

  it('"last night" / "overnight"', () => {
    expect(q('last night').sinceMs).toBe(14 * 60 * 60 * 1000);
    expect(q('overnight').sinceMs).toBe(14 * 60 * 60 * 1000);
  });

  it('"this morning"', () => {
    expect(q('this morning').sinceMs).toBe(12 * 60 * 60 * 1000);
  });

  it('"this week" / "past week" / "last week"', () => {
    expect(q('this week').sinceMs).toBe(7 * 24 * 60 * 60 * 1000);
    expect(q('past week').sinceMs).toBe(7 * 24 * 60 * 60 * 1000);
    expect(q('last week').sinceMs).toBe(7 * 24 * 60 * 60 * 1000);
  });

  it('"this month"', () => {
    expect(q('this month').sinceMs).toBe(30 * 24 * 60 * 60 * 1000);
  });
});

describe('queryParser — triage keywords', () => {
  it('spiking', () => {
    expect(q('spiking').spiking).toBe(true);
  });

  it('new / fresh', () => {
    expect(q('new').newOnly).toBe(true);
    expect(q('fresh').newOnly).toBe(true);
  });

  it('stale / old', () => {
    expect(q('stale').staleOnly).toBe(true);
    expect(q('old').staleOnly).toBe(true);
  });
});

describe('queryParser — sort + limit', () => {
  it('bare alias "noisy" → sort:count', () => {
    expect(q('noisy').sort).toBe('count');
  });

  it('bare alias "newest" → sort:recent', () => {
    expect(q('newest').sort).toBe('recent');
  });

  it('field-prefix sort:count', () => {
    expect(q('sort:count').sort).toBe('count');
  });

  it('"top 10" sets sort + limit', () => {
    const r = q('top 10');
    expect(r.sort).toBe('count');
    expect(r.limit).toBe(10);
  });

  it('"first 50" sets only limit', () => {
    const r = q('first 50');
    expect(r.sort).toBe(null);
    expect(r.limit).toBe(50);
  });

  it('"most recent 100" sets sort + limit', () => {
    const r = q('most recent 100');
    expect(r.sort).toBe('recent');
    expect(r.limit).toBe(100);
  });

  it('"most frequent" without N sets only sort', () => {
    const r = q('most frequent');
    expect(r.sort).toBe('count');
    expect(r.limit).toBe(null);
  });

  it('bare integer after sort reads as limit', () => {
    const r = q('noisy 25');
    expect(r.sort).toBe('count');
    expect(r.limit).toBe(25);
  });

  it('"limit:25" / "limit 25"', () => {
    expect(q('limit:25').limit).toBe(25);
    expect(q('limit 25').limit).toBe(25);
  });
});

describe('queryParser — text + wildcards', () => {
  it('quoted phrase becomes text', () => {
    expect(q('"connection timeout"').text).toEqual([
      { value: 'connection timeout', isPattern: false }
    ]);
  });

  it('wildcard outside quotes', () => {
    expect(q('timeout*').text).toEqual([{ value: 'timeout', isPattern: true }]);
  });

  it('wildcard inside quotes', () => {
    expect(q('"path/api/*"').text).toEqual([{ value: 'path/api/', isPattern: true }]);
  });

  it('text:foo prefix is always text', () => {
    expect(q('text:database').text).toEqual([{ value: 'database', isPattern: false }]);
  });

  it('numeric plural strip', () => {
    expect(q('500s').words).toEqual(['500']);
  });

  it('unrecognised word becomes free-text word', () => {
    expect(q('OOM').words).toEqual(['OOM']);
  });
});

describe('queryParser — tags + URLs', () => {
  it('tag:KEY:VALUE', () => {
    expect(q('tag:os:windows').tags).toEqual([{ key: 'os', values: ['windows'] }]);
  });

  it('tag:KEY:VAL,VAL (comma-OR)', () => {
    expect(q('tag:browser:chrome,firefox').tags).toEqual([
      { key: 'browser', values: ['chrome', 'firefox'] }
    ]);
  });

  it('tag:VALUE (loose, key=null)', () => {
    expect(q('tag:windows').tags).toEqual([{ key: null, values: ['windows'] }]);
  });

  it('url:/path', () => {
    expect(q('url:/login').urls).toEqual([{ value: '/login', isPattern: false }]);
  });

  it('url:*/api/*', () => {
    expect(q('url:*/api/*').urls).toEqual([{ value: '/api/', isPattern: true }]);
  });

  it('url:"quoted"', () => {
    expect(q('url:"https://example.com/login"').urls).toEqual([
      { value: 'https://example.com/login', isPattern: false }
    ]);
  });
});

describe('queryParser — saved filters + metacommands', () => {
  it('@name → recallSaved', () => {
    expect(q('@critical').recallSaved).toBe('critical');
  });

  it('"save as :name"', () => {
    expect(q('fatal save as :urgent').saveAs).toBe('urgent');
  });

  it('"forget @name" / "unsave @name"', () => {
    expect(q('forget @critical').forgetName).toBe('critical');
    expect(q('unsave @critical').forgetName).toBe('critical');
  });

  it('-@name → negSavedNames', () => {
    expect([...q('-@critical').negSavedNames]).toEqual(['critical']);
  });
});

describe('queryParser — filler words', () => {
  it('"issues" / "events" silently dropped', () => {
    const r = q('errors today issues');
    expect(r.words).toEqual([]); // "issues" did not become a substring word
    expect([...r.levels]).toEqual(['error']);
    expect(r.sinceMs).toBe(24 * 60 * 60 * 1000);
  });
});

describe('queryParser — composition', () => {
  it('"crashes overnight"', () => {
    const r = q('crashes overnight');
    expect([...r.levels]).toEqual(['fatal']);
    expect(r.sinceMs).toBe(14 * 60 * 60 * 1000);
  });

  it('"top 10 errors today"', () => {
    const r = q('top 10 errors today');
    expect(r.sort).toBe('count');
    expect(r.limit).toBe(10);
    expect([...r.levels]).toEqual(['error']);
    expect(r.sinceMs).toBe(24 * 60 * 60 * 1000);
  });

  it('"most recent 100 issues"', () => {
    const r = q('most recent 100 issues');
    expect(r.sort).toBe('recent');
    expect(r.limit).toBe(100);
    expect(r.words).toEqual([]); // "issues" filtered as filler
  });

  it('"level:fatal,error noisy"', () => {
    const r = q('level:fatal,error noisy');
    expect([...r.levels].sort()).toEqual(['error', 'fatal']);
    expect(r.sort).toBe('count');
  });

  it('"tag:browser:chrome url:/login fatal 5m"', () => {
    const r = q('tag:browser:chrome url:/login fatal 5m');
    expect(r.tags).toEqual([{ key: 'browser', values: ['chrome'] }]);
    expect(r.urls).toEqual([{ value: '/login', isPattern: false }]);
    expect([...r.levels]).toEqual(['fatal']);
    expect(r.sinceMs).toBe(5 * 60 * 1000);
  });
});

describe('queryParser — token spans (highlighter contract)', () => {
  // The spans matter because the input overlay renders a colour for
  // each character within a token. Overlapping/duplicate spans cause
  // the highlighter to paint the same range twice — so we explicitly
  // test that adjacent multi-token phrases like "top 10" use
  // non-overlapping spans.
  it('"top 10" emits sort + limit with non-overlapping spans', () => {
    const tokens = tokenize('top 10');
    expect(tokens).toHaveLength(2);
    expect(tokens[0]?.kind).toBe('sort');
    expect(tokens[0]?.span).toEqual([0, 3]);
    expect(tokens[1]?.kind).toBe('limit');
    expect(tokens[1]?.span).toEqual([4, 6]);
  });

  it('"most recent 100" emits sort + limit with non-overlapping spans', () => {
    const tokens = tokenize('most recent 100');
    expect(tokens).toHaveLength(2);
    expect(tokens[0]?.kind).toBe('sort');
    expect(tokens[0]?.span).toEqual([0, 11]);
    expect(tokens[1]?.kind).toBe('limit');
    expect(tokens[1]?.span).toEqual([12, 15]);
  });
});
