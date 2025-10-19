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
  let res = await fetcher(new Request('http://test/auth/register', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ email, username, password })
  }));
  const json = await res.json();
  return { email, username, password, token: json.token as string };
}

describe('Account', () => {
  test('update email and username', async () => {
    const { token } = await register();
    const newEmail = `n${Math.random().toString(36).slice(2)}@t.com`;
    const newUsername = `nu_${Math.random().toString(36).slice(2, 8)}`;
    const res = await fetcher(new Request('http://test/auth/account', {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
      body: JSON.stringify({ email: newEmail, username: newUsername })
    }));
    expect(res.status).toBe(200);
    const json = await res.json();
    expect(json.ok).toBe(true);
  });

  test('change password then login with new password', async () => {
    const { token, email, password } = await register();
    const newPassword = 'N3wP@ssw0rd!';
    let res = await fetcher(new Request('http://test/auth/account/password', {
      method: 'POST', headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` }, body: JSON.stringify({ currentPassword: password, newPassword })
    }));
    expect(res.status).toBe(200);
    let json = await res.json();
    expect(json.ok).toBe(true);

    // login with new pass
    res = await fetcher(new Request('http://test/auth/login', {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ identifier: email, password: newPassword })
    }));
    expect(res.status).toBe(200);
  });
});


