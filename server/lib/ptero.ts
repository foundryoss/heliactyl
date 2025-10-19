import type { ServerConfig, EggRecord } from '../../types/index';

export interface PteroClient {
  createUser(payload: { email: string; username: string; first_name: string; last_name: string; password: string }): Promise<{ id: number }>;
  updateUser(id: number, payload: Partial<{ email: string; username: string; first_name: string; last_name: string; password: string }>): Promise<void>;
  deleteUser(id: number): Promise<void>;
  createServer(spec: any): Promise<{ id: number; uuid: string; identifier: string; name: string }>;
  deleteServer(serverId: number): Promise<void>;
  fetchAllServers(): Promise<any[]>;
  fetchEggs(): Promise<EggRecord[]>;
  getWebSocketCredentials(serverIdentifier: string): Promise<{ token: string; socket: string }>;
}

export function createPteroClient(cfg: ServerConfig): PteroClient {
  const base = cfg.pterodactyl?.url?.replace(/\/$/, '') || '';
  const apiKey = cfg.pterodactyl?.apiKey || '';
  const clientKey = cfg.pterodactyl?.clientKey || '';
  const headers = { 
    'Content-Type': 'application/json', 
    'Accept': 'Application/vnd.pterodactyl.v1+json',
    'Authorization': `Bearer ${apiKey}` 
  };
  const clientHeaders = { 
    'Content-Type': 'application/json', 
    'Accept': 'application/json',
    'Authorization': `Bearer ${clientKey}` 
  };

  async function appGet(path: string) {
    const res = await fetch(`${base}/api/application${path}`, { headers });
    if (!res.ok) throw new Error(`Ptero GET ${path} failed: ${res.status}`);
    return res.json();
  }
  async function appPost(path: string, body: any) {
    const url = `${base}/api/application${path}`;
    console.log('[Ptero] Request URL:', url);
    console.log('[Ptero] Headers:', headers);
    console.log('[Ptero] Request body:', JSON.stringify(body, null, 2));
    
    const res = await fetch(url, { method: 'POST', headers, body: JSON.stringify(body) });
    console.log('[Ptero] Response status:', res.status);
    console.log('[Ptero] Response headers:', Object.fromEntries(res.headers.entries()));
    
    if (!res.ok) {
      const text = await res.text();
      throw new Error(`Ptero POST ${path} failed: ${res.status} - ${text}`);
    }
    const contentType = res.headers.get('content-type') || '';
    if (!contentType.includes('application/json')) {
      const text = await res.text();
      throw new Error(`Ptero POST ${path} returned non-JSON response: ${text}`);
    }
    return res.json();
  }
  async function appPatch(path: string, body: any) {
    const res = await fetch(`${base}/api/application${path}`, { method: 'PATCH', headers, body: JSON.stringify(body) });
    if (!res.ok) throw new Error(`Ptero PATCH ${path} failed: ${res.status}`);
    return res.json().catch(() => ({}));
  }
  async function appDelete(path: string) {
    const res = await fetch(`${base}/api/application${path}`, { method: 'DELETE', headers });
    if (!res.ok) throw new Error(`Ptero DELETE ${path} failed: ${res.status}`);
  }

  async function clientGet(path: string) {
    const res = await fetch(`${base}/api/client${path}`, { headers: clientHeaders });
    if (!res.ok) throw new Error(`Ptero Client GET ${path} failed: ${res.status}`);
    const contentType = res.headers.get('content-type') || '';
    if (!contentType.includes('application/json')) {
      const text = await res.text();
      throw new Error(`Ptero Client GET ${path} returned non-JSON response: ${text}`);
    }
    return res.json();
  }

  return {
    async createUser(payload) {
      const json = await appPost('/users', payload);
      return { id: json?.attributes?.id ?? json?.id };
    },
    async updateUser(id, payload) {
      await appPatch(`/users/${id}`, payload);
    },
    async deleteUser(id) {
      await appDelete(`/users/${id}`);
    },
    async createServer(spec) {
      const json = await appPost('/servers', spec);
      return { 
        id: json?.attributes?.id ?? json?.id, 
        uuid: json?.attributes?.uuid ?? json?.uuid, 
        identifier: json?.attributes?.identifier ?? json?.identifier,
        name: json?.attributes?.name ?? json?.name 
      };
    },
    async deleteServer(serverId) {
      await appDelete(`/servers/${serverId}`);
    },
    async fetchAllServers() {
      const json = await appGet('/servers?page=1&per_page=100000');
      return json?.data?.map((d: any) => d.attributes) ?? [];
    },
    async fetchEggs() {
      const nestsJson = await appGet('/nests?page=1&per_page=100000');
      const nests = nestsJson?.data ?? [];
      const eggs: EggRecord[] = [];
      for (const n of nests) {
        const nestId = n.attributes.id;
        const ejson = await appGet(`/nests/${nestId}/eggs?page=1&per_page=100000&include=variables`);
        for (const e of ejson?.data ?? []) {
          const a = e.attributes;
          console.log(a)
          console.log(`[Eggs Sync] Egg ${a.id} (${a.name}) environment:`, a.relationships.variables.data);
          eggs.push({
            id: crypto.randomUUID(),
            nestId,
            eggId: a.id,
            name: a.name,
            dockerImage: a.docker_image,
            startup: a.startup,
            environment: a?.environment || {},
          });
        }
      }
      return eggs;
    },
    async getWebSocketCredentials(serverIdentifier) {
      const json = await clientGet(`/servers/${serverIdentifier}/websocket`);
      return {
        token: json.data?.token || json.token,
        socket: json.data?.socket || json.socket
      };
    },
  };
}


