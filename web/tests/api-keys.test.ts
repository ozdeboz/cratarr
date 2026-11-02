// API keys kept with what uses them (lib/apiKeys): MCP's address from the server's own, and each connection's keys only.
import { test } from 'node:test';
import assert from 'node:assert/strict';

const { keysFor, lastUsed, mcpAddress } = await import('../src/lib/apiKeys.ts');

const key = (id: string, purpose: 'home_assistant' | 'mcp' | null, last_used: number | null = null) => ({ id, name: id, created: 1, last_used, purpose });

test("MCP's address is the server's own, its /mcp", () => {
  assert.equal(mcpAddress('https://music.example.org'), 'https://music.example.org/mcp');
  assert.equal(mcpAddress('http://192.0.2.10:8081/'), 'http://192.0.2.10:8081/mcp');
});

test("a connection's keys are its own: MCP's never Home Assistant's, nor one made without a purpose", () => {
  const keys = [key('a', 'home_assistant'), key('b', 'mcp'), key('c', null), key('d', 'mcp')];
  assert.deepEqual(keysFor(keys, 'mcp').map((k) => k.id), ['b', 'd']);
  assert.deepEqual(keysFor(keys, 'home_assistant').map((k) => k.id), ['a']);
});

test('last used: the latest of them, 0 where none was', () => {
  assert.equal(lastUsed([key('a', 'mcp', 30), key('b', 'mcp', 90), key('c', 'mcp')]), 90);
  assert.equal(lastUsed([key('a', 'mcp')]), 0);
  assert.equal(lastUsed([]), 0);
});
