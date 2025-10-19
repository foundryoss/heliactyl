export interface OAuthProfile {
  id: string;
  email: string | null;
  username: string | null;
}

export interface OAuthProvider {
  name: string;
  getAuthorizationUrl(state: string): string;
  exchangeCode(code: string): Promise<{ accessToken: string; expiresIn?: number }>;
  fetchProfile(accessToken: string): Promise<OAuthProfile>;
}


