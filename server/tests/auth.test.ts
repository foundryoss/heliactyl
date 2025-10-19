import { describe, expect, test, beforeAll } from 'bun:test';
import { makeTestApp } from './helpers';

let fetcher: (input: RequestInfo, init?: RequestInit) => Promise<Response>;

beforeAll(async () => {
  const { app } = await makeTestApp();
  fetcher = app.fetch;
});

describe('Auth module', () => {
  test('register -> login -> me', async () => {
    const email = `u${Math.random().toString(36).slice(2)}@t.com`;
    const username = `user_${Math.random().toString(36).slice(2, 8)}`;
    const password = 'P@ssw0rd123';

    // register
    let res = await fetcher(new Request('http://test/auth/register', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email, username, password })
    }));
    expect(res.status).toBe(201);
    let json = await res.json();
    expect(json.token).toBeDefined();
    const token = json.token as string;

    // me
    res = await fetcher(new Request('http://test/auth/me', { headers: { Authorization: `Bearer ${token}` } }));
    expect(res.status).toBe(200);
    json = await res.json();
    expect(json.user).toBeTruthy();
    expect(json.user.email).toBe(email);

    // logout
    res = await fetcher(new Request('http://test/auth/logout', { method: 'POST', headers: { Authorization: `Bearer ${token}` } }));
    expect(res.status).toBe(200);

    // login
    res = await fetcher(new Request('http://test/auth/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ identifier: email, password })
    }));
    expect(res.status).toBe(200);
    json = await res.json();
    expect(json.token).toBeDefined();
  });
});


