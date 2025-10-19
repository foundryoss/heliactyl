import { describe, expect, test, beforeAll } from 'bun:test';
import { makeTestApp } from './helpers';

let fetcher: (input: RequestInfo, init?: RequestInit) => Promise<Response>;

beforeAll(async () => {
  const { app } = await makeTestApp();
  fetcher = app.fetch;
});

async function register() {
  const email = `u${Math.random().toString(36).slice(2)}@t.com`;
  const username = `user_${Math.random().toString(36).slice(2, 8)}`;
  const password = 'P@ssw0rd123';
  const res = await fetcher(new Request('http://test/auth/register', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ email, username, password })
  }));
  expect(res.status).toBe(201);
  const json = await res.json();
  return { email, username, password, token: json.token as string };
}

describe('Tenants', () => {
  test('list + resources + members + create/delete', async () => {
    const { token, email } = await register();

    // list
    let res = await fetcher(new Request('http://test/tenants', { headers: { Authorization: `Bearer ${token}` } }));
    expect(res.status).toBe(200);
    let json = await res.json();
    expect(Array.isArray(json.items)).toBe(true);
    const defaultTenant = json.items[0];
    expect(defaultTenant).toBeTruthy();

    // resources
    res = await fetcher(new Request(`http://test/tenants/${defaultTenant.id}/resources`, { headers: { Authorization: `Bearer ${token}` } }));
    expect(res.status).toBe(200);

    // members
    res = await fetcher(new Request(`http://test/tenants/${defaultTenant.id}/members`, { headers: { Authorization: `Bearer ${token}` } }));
    expect(res.status).toBe(200);
    json = await res.json();
    expect(Array.isArray(json.items)).toBe(true);

    // add-by-email (idempotent)
    res = await fetcher(new Request(`http://test/tenants/${defaultTenant.id}/members/by-email`, {
      method: 'POST', headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` }, body: JSON.stringify({ email })
    }));
    expect(res.status).toBe(200);

    // create second tenant
    res = await fetcher(new Request('http://test/tenants', {
      method: 'POST', headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` }, body: JSON.stringify({ name: 'CI Tenant' })
    }));
    expect(res.status).toBe(200);
    json = await res.json();
    const newTenantId = json.id as string;
    expect(newTenantId).toBeDefined();

    // delete new tenant (cleanup)
    res = await fetcher(new Request(`http://test/tenants/${newTenantId}`, { method: 'DELETE', headers: { Authorization: `Bearer ${token}` } }));
    expect(res.status).toBe(200);
  });

  test('tenant audit endpoint enforces membership', async () => {
    const owner = await register();
    let res = await fetcher(new Request('http://test/tenants', { headers: { Authorization: `Bearer ${owner.token}` } }));
    expect(res.status).toBe(200);
    let json = await res.json();
    const tenantId = json.items[0].id as string;

    res = await fetcher(new Request(`http://test/tenants/${tenantId}/audit`, { headers: { Authorization: `Bearer ${owner.token}` } }));
    expect(res.status).toBe(200);
    json = await res.json();
    expect(Array.isArray(json.items)).toBe(true);

    const outsider = await register();
    res = await fetcher(new Request(`http://test/tenants/${tenantId}/audit`, { headers: { Authorization: `Bearer ${outsider.token}` } }));
    expect(res.status).toBe(403);
  });
});


