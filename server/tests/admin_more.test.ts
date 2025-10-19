import { describe, expect, test, beforeAll } from 'bun:test';
import { makeTestApp } from './helpers';

let fetcher: (input: RequestInfo, init?: RequestInit) => Promise<Response>;

beforeAll(async () => {
  const { app } = await makeTestApp();
  fetcher = app.fetch;
});

async function adminToken() {
  const email = `adm${Math.random().toString(36).slice(2)}@t.com`;
  const username = `admin_${Math.random().toString(36).slice(2, 8)}`;
  const password = 'P@ssw0rd123';
  const res = await fetcher(new Request('http://test/auth/register', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ email, username, password })
  }));
  const json = await res.json();
  return json.token as string;
}

describe('Admin more', () => {
  test('diagnostics and eggs list', async () => {
    const token = await adminToken();
    let res = await fetcher(new Request('http://test/admin/diagnostics', { headers: { Authorization: `Bearer ${token}` } }));
    expect(res.status).toBe(200);
    res = await fetcher(new Request('http://test/admin/eggs', { headers: { Authorization: `Bearer ${token}` } }));
    expect(res.status).toBe(200);
  });
});


