import { describe, expect, test, beforeAll } from 'bun:test';
import { makeTestApp } from './helpers';

let fetcher: (input: RequestInfo, init?: RequestInit) => Promise<Response>;

beforeAll(async () => {
  const { app } = await makeTestApp();
  fetcher = app.fetch;
});

async function loginAsFirstAdmin() {
  // First registration becomes admin in our implementation
  const email = `adm${Math.random().toString(36).slice(2)}@t.com`;
  const username = `admin_${Math.random().toString(36).slice(2, 8)}`;
  const password = 'P@ssw0rd123';
  const res = await fetcher(new Request('http://test/auth/register', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ email, username, password })
  }));
  const json = await res.json();
  return json.token as string;
}

describe('Admin', () => {
  test('admin users list and packages view', async () => {
    const token = await loginAsFirstAdmin();
    let res = await fetcher(new Request('http://test/admin/users', { headers: { Authorization: `Bearer ${token}` } }));
    expect(res.status).toBe(200);
    res = await fetcher(new Request('http://test/admin/packages', { headers: { Authorization: `Bearer ${token}` } }));
    expect(res.status).toBe(200);
  });
});


