// API keys kept with what uses them (the owner, 2026-10-24: under Connections, not Authentication): Home Assistant's
// and MCP's (0.13, decision 0090), pure so the tests take them as they are.
import type { ApiKey, KeyPurpose } from './api';

/** The keys made for `purpose`. */
export const keysFor = (keys: readonly ApiKey[], purpose: KeyPurpose): ApiKey[] => keys.filter((k) => k.purpose === purpose);

/** Where an assistant reaches Cratarr over MCP: this server's own address, its `/mcp`. */
export const mcpAddress = (origin: string) => `${origin.replace(/\/+$/, '')}/mcp`;

/** The latest any of them was used (epoch s), or 0. */
export const lastUsed = (keys: readonly ApiKey[]) => Math.max(0, ...keys.map((k) => k.last_used ?? 0));
