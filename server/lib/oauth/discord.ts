import type { OAuthProvider, OAuthProfile } from './types';
import type { ServerConfig } from '../../../types/index';

export function createDiscordProvider(cfg: ServerConfig): OAuthProvider | null {
  const disc = cfg.oauth?.discord;
  if (!disc?.enabled) return null;

  const base = 'https://discord.com/api';
  const name = 'discord';
  const clientId = disc.clientId;
  const clientSecret = disc.clientSecret;
  const redirectUri = disc.redirectUri;
  const scopes = disc.scopes?.length ? disc.scopes : ['identify', 'email'];

  function getAuthorizationUrl(state: string): string {
    const url = new URL(base + '/oauth2/authorize');
    url.searchParams.set('response_type', 'code');
    url.searchParams.set('client_id', clientId);
    url.searchParams.set('scope', scopes.join(' '));
    url.searchParams.set('state', state);
    url.searchParams.set('redirect_uri', redirectUri);
    url.searchParams.set('prompt', 'consent');
    return url.toString();
  }

  async function exchangeCode(code: string): Promise<{ accessToken: string; expiresIn?: number }> {
    const body = new URLSearchParams({
      client_id: clientId,
      client_secret: clientSecret,
      grant_type: 'authorization_code',
      code,
      redirect_uri: redirectUri,
    });
    const res = await fetch(base + '/oauth2/token', {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body,
    });
    if (!res.ok) throw new Error('Discord token exchange failed');
    const json = await res.json();
    return { accessToken: json.access_token, expiresIn: json.expires_in };
  }

  async function fetchProfile(accessToken: string): Promise<OAuthProfile> {
    const res = await fetch(base + '/users/@me', {
      headers: { Authorization: `Bearer ${accessToken}` },
    });
    if (!res.ok) throw new Error('Discord profile fetch failed');
    const u = await res.json();
    const email = u.email ?? null;
    const username = u.global_name || u.username || null;
    return { id: String(u.id), email, username };
  }

  return { name, getAuthorizationUrl, exchangeCode, fetchProfile };
}


