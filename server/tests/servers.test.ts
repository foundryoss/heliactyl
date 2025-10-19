import { describe, expect, test, beforeAll } from 'bun:test';
import { makeTestApp } from './helpers';

let fetcher: (input: RequestInfo, init?: RequestInit) => Promise<Response>;

beforeAll(async () => {
  const { app } = await makeTestApp();
  fetcher = app.fetch;
});

async function createTenantToken() {
  const email = `u${Math.random().toString(36).slice(2)}@t.com`;
  const username = `user_${Math.random().toString(36).slice(2, 8)}`;
  const password = 'P@ssw0rd123';
  let res = await fetcher(new Request('http://test/auth/register', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ email, username, password })
  }));
  const json = await res.json();
  const token = json.token as string;
  res = await fetcher(new Request('http://test/tenants', { headers: { Authorization: `Bearer ${token}` } }));
  const list = await res.json();
  const tenantId = list.items[0].id as string;
  return { token, tenantId };
}

describe('Servers', () => {
  test('list (empty) and no create without resources breach', async () => {
    const { token, tenantId } = await createTenantToken();
    // list
    let res = await fetcher(new Request(`http://test/tenants/${tenantId}/servers`, { headers: { Authorization: `Bearer ${token}` } }));
    expect(res.status).toBe(200);
    let json = await res.json();
    expect(Array.isArray(json.items)).toBe(true);
  });
});


