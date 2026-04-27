// Lifecycle of the singleton WebSocket. The bug it pins: connect(X) followed
// by connect(X) used to tear down the in-flight socket and open a new one,
// producing the browser warning "WebSocket is closed before the connection
// is established" and wasting the handshake. After the fix, same-project
// re-calls are no-ops; cross-project still tears down and reopens.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

class MockWebSocket {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSING = 2;
  static readonly CLOSED = 3;
  static instances: MockWebSocket[] = [];
  url: string;
  readyState: number = MockWebSocket.CONNECTING;
  onopen: ((ev: Event) => void) | null = null;
  onclose: ((ev: CloseEvent) => void) | null = null;
  onerror: ((ev: Event) => void) | null = null;
  onmessage: ((ev: MessageEvent) => void) | null = null;
  closed = false;
  constructor(url: string) {
    this.url = url;
    MockWebSocket.instances.push(this);
  }
  close() {
    this.closed = true;
    if (
      this.readyState === MockWebSocket.CONNECTING ||
      this.readyState === MockWebSocket.OPEN
    ) {
      this.readyState = MockWebSocket.CLOSED;
      this.onclose?.({} as CloseEvent);
    }
  }
}

beforeEach(() => {
  MockWebSocket.instances = [];
  vi.stubGlobal('WebSocket', MockWebSocket);
  // Module-level state in ws.ts (socket, pendingProject, reconnectTimer)
  // must reset between tests, so we re-import per case.
  vi.resetModules();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

async function loadWs() {
  const stores = await import('./stores.svelte');
  stores.projects.current = 'default';
  const ws = await import('./ws');
  return ws;
}

// Small accessor so each test reads a concrete `MockWebSocket` instead of
// `MockWebSocket | undefined`. TS strict (`noUncheckedIndexedAccess`)
// would otherwise force `?.` at every callsite, which obscures the
// invariant the assertions are about to prove.
function inst(n: number): MockWebSocket {
  const ws = MockWebSocket.instances[n];
  if (!ws) throw new Error(`no MockWebSocket instance at index ${n}`);
  return ws;
}

describe('ws.connect', () => {
  it('opens a single WebSocket on first call', async () => {
    const { connect } = await loadWs();
    connect('foo');
    expect(MockWebSocket.instances).toHaveLength(1);
    expect(inst(0).url).toMatch(/\/ws\/foo$/);
    expect(inst(0).closed).toBe(false);
  });

  it('is a no-op when called again with the same project while CONNECTING', async () => {
    const { connect } = await loadWs();
    connect('foo');
    connect('foo');
    expect(MockWebSocket.instances).toHaveLength(1);
    expect(inst(0).closed).toBe(false);
  });

  it('is a no-op when called again with the same project while OPEN', async () => {
    const { connect } = await loadWs();
    connect('foo');
    inst(0).readyState = MockWebSocket.OPEN;
    connect('foo');
    expect(MockWebSocket.instances).toHaveLength(1);
    expect(inst(0).closed).toBe(false);
  });

  it('tears down the existing socket and opens a new one when switching projects', async () => {
    const { connect } = await loadWs();
    connect('foo');
    connect('bar');
    expect(MockWebSocket.instances).toHaveLength(2);
    expect(inst(0).closed).toBe(true);
    expect(inst(1).url).toMatch(/\/ws\/bar$/);
    expect(inst(1).closed).toBe(false);
  });

  it('reopens after disconnect()', async () => {
    const { connect, disconnect } = await loadWs();
    connect('foo');
    disconnect();
    expect(inst(0).closed).toBe(true);
    connect('foo');
    expect(MockWebSocket.instances).toHaveLength(2);
    expect(inst(1).closed).toBe(false);
  });
});
