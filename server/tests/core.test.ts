import { describe, expect, test, beforeAll } from 'bun:test';
import { makeTestApp } from './helpers';

let fetcher: (input: RequestInfo, init?: RequestInit) => Promise<Response>;

beforeAll(async () => {
  const { app } = await makeTestApp();
  fetcher = app.fetch;
});

describe('Core module', () => {
  test('health returns ok', async () => {
    const res = await fetcher(new Request('http://test/core/health'));
    expect(res.status).toBe(200);
    const json = await res.json();
    expect(json.ok).toBe(true);
  });

  test('info returns name and version', async () => {
    const res = await fetcher(new Request('http://test/core/info'));
    expect(res.status).toBe(200);
    const json = await res.json();
    expect(json.name).toBeDefined();
    expect(json.version).toBeDefined();
  });
});


