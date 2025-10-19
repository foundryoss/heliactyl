import { describe, expect, test, beforeAll } from 'bun:test';
import { makeTestApp } from './helpers';

let fetcher: (input: RequestInfo, init?: RequestInit) => Promise<Response>;

beforeAll(async () => {
  const { app } = await makeTestApp();
  fetcher = app.fetch;
});

async function registerAndLogin() {
  const email = `u${Math.random().toString(36).slice(2)}@t.com`;
  const username = `user_${Math.random().toString(36).slice(2, 8)}`;
  const password = 'P@ssw0rd123';
  let res = await fetcher(new Request('http://test/auth/register', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ email, username, password })
  }));
  expect(res.status).toBe(201);
  let json = await res.json();
  const token = json.token as string;
  return { email, username, password, token };
}

describe('Sessions & Audit', () => {
  test('list sessions and revoke one', async () => {
    const { token } = await registerAndLogin();

    // list sessions
    let res = await fetcher(new Request('http://test/auth/sessions', { headers: { Authorization: `Bearer ${token}` } }));
    expect(res.status).toBe(200);
    let json = await res.json();
    expect(Array.isArray(json.items)).toBe(true);
    expect(json.items.length).toBeGreaterThanOrEqual(1);
    const current = json.items.find((s: any) => s.isCurrent);
    expect(current).toBeTruthy();

    // revoke current session
    res = await fetcher(new Request(`http://test/auth/sessions/${current.id}`, { method: 'DELETE', headers: { Authorization: `Bearer ${token}` } }));
    expect(res.status).toBe(200);
    json = await res.json();
    expect(json.ok).toBe(true);

    // session should be invalid now
    res = await fetcher(new Request('http://test/auth/me', { headers: { Authorization: `Bearer ${token}` } }));
    expect(res.status).toBe(401);
  });

  test('audit log lists recent actions with pagination', async () => {
    const { email, password, token } = await registerAndLogin();

    // list audit logs - should include register & success
    let res = await fetcher(new Request('http://test/auth/audit?page=1&pageSize=10', { headers: { Authorization: `Bearer ${token}` } }));
    expect(res.status).toBe(200);
    let json = await res.json();
    const actions = json.items.map((i: any) => i.action);
    expect(actions.includes('auth:register')).toBe(true);
    expect(actions.includes('auth:success')).toBe(true);

    // logout
    res = await fetcher(new Request('http://test/auth/logout', { method: 'POST', headers: { Authorization: `Bearer ${token}` } }));
    expect(res.status).toBe(200);

    // login again
    res = await fetcher(new Request('http://test/auth/login', {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ identifier: email, password })
    }));
    expect(res.status).toBe(200);
    const loginJson = await res.json();
    const token2 = loginJson.token as string;

    // verify logout appears
    res = await fetcher(new Request('http://test/auth/audit?page=1&pageSize=10', { headers: { Authorization: `Bearer ${token2}` } }));
    expect(res.status).toBe(200);
    json = await res.json();
    const actions2 = json.items.map((i: any) => i.action);
    expect(actions2.includes('auth:logout')).toBe(true);
    expect(json.meta.page).toBe(1);
    expect(json.meta.pageSize).toBe(10);
  });
});


