import { useMemo } from 'react'
import { useApp } from '@/providers/AppProvider'

export interface ApiError { status: number; message: string }

function joinUrl(base: string, path: string) {
	if (base.startsWith('/')) return base + (base.endsWith('/') ? '' : '') + path.replace(/^\//, '')
	const url = new URL(path, base)
	return url.toString()
}

export function createApi(baseUrl: string, getToken: () => string | null) {
	async function request<T>(path: string, init?: RequestInit): Promise<T> {
		const headers: Record<string, string> = { 'content-type': 'application/json' }
		const token = getToken()
		if (token) headers['authorization'] = `Bearer ${token}`
		const url = baseUrl.startsWith('/') ? joinUrl(baseUrl, path) : new URL(path, baseUrl).toString()
		const res = await fetch(url, { ...init, headers: { ...headers, ...(init?.headers as any) } })
		const isJson = res.headers.get('content-type')?.includes('application/json')
		if (!res.ok) {
			const body = isJson ? await res.json().catch(() => ({})) : { error: await res.text().catch(() => '') }
			const message = String((body as any).error || (body as any).message || res.statusText)
			throw { status: res.status, message } as ApiError
		}
		return (isJson ? res.json() : (undefined as any)) as Promise<T>
	}

	return {
		auth: {
			register: (data: { email: string; username: string; password: string }) => request<{ token: string; userId: string }>('api/auth/register', { method: 'POST', body: JSON.stringify(data) }),
			login: (data: { identifier: string; password: string }) => request<{ token: string; userId: string }>('api/auth/login', { method: 'POST', body: JSON.stringify(data) }),
			logout: () => request<{ ok: boolean }>('api/auth/logout', { method: 'POST' }),
			me: () => request<{ user: { id: string; email: string; username: string | null; discordId: string | null; isAdmin?: boolean; createdAt: string; updatedAt: string } | null }>('api/user/me', { method: 'GET' }),
		},
		authSessions: {
			list: (page = 1, pageSize = 20) => request<{ items: Array<{ id: string; createdAt: string; expiresAt: string; isCurrent: boolean; ip: string | null; userAgent: string | null }>; meta: any }>(`api/auth/sessions?page=${page}&pageSize=${pageSize}`),
			delete: (id: string) => request<{ ok: boolean }>(`api/auth/sessions/${id}`, { method: 'DELETE' }),
		},
		account: {
			update: (data: { email?: string; username?: string }) => request<{ ok: boolean }>('api/auth/account', { method: 'PATCH', body: JSON.stringify(data) }),
			changePassword: (data: { currentPassword: string; newPassword: string }) => request<{ ok: boolean }>('api/auth/account/password', { method: 'POST', body: JSON.stringify(data) }),
			delete: () => request<{ ok: boolean }>('api/auth/account/delete', { method: 'DELETE' }),
			audit: (page = 1, pageSize = 20) => request<{ items: any[]; meta: any }>('api/auth/audit?page=' + page + '&pageSize=' + pageSize),
		},
		core: {
			health: () => request<{ ok: boolean }>('api/core/health'),
			info: () => request<{ name?: string; version?: string }>('api/core/info'),
			locations: () => request<{ items: Array<{ id: number; slug: string; name: string; lockedPackage?: string }> }>('api/core/locations'),
			eggs: () => request<{ items: Array<{ eggId: number; name: string; dockerImage: string }> }>('api/core/eggs'),
			mqttConfig: () => request<{ url: string; username: string; password: string; encryptionKey: string; signingKey: string }>('api/core/mqtt/config'),
		},
		tenants: {
			list: () => request<{ items: Array<any> }>('api/tenants'),
			create: (data: { name: string }) => request<{ id: string; name: string }>('api/tenants', { method: 'POST', body: JSON.stringify(data) }),
			members: (tenantId: string) => request<{ items: Array<any> }>(`api/tenants/${tenantId}/members`),
			audit: (tenantId: string, page = 1, pageSize = 10) => request<{ items: any[]; meta: any }>(`api/tenants/${tenantId}/audit?page=${page}&pageSize=${pageSize}`),
			addMember: (tenantId: string, userId: string) => request<{ ok: boolean }>(`api/tenants/${tenantId}/members`, { method: 'POST', body: JSON.stringify({ userId }) }),
			addMemberByEmail: (tenantId: string, email: string) => request<{ ok: boolean }>(`api/tenants/${tenantId}/members/by-email`, { method: 'POST', body: JSON.stringify({ email }) }),
			removeMember: (tenantId: string, userId: string) => request<{ ok: boolean }>(`api/tenants/${tenantId}/members/${userId}`, { method: 'DELETE' }),
			transfer: (tenantId: string, userId: string) => request<{ ok: boolean }>(`api/tenants/${tenantId}/transfer`, { method: 'POST', body: JSON.stringify({ userId }) }),
			resources: (tenantId: string) => request<{ package: any; extra: any; used: any; remaining: any }>(`api/tenants/${tenantId}/resources`),
			delete: (tenantId: string) => request<{ ok: boolean }>(`api/tenants/${tenantId}`, { method: 'DELETE' }),
		},
		servers: {
			list: (tenantId: string) => request<{ items: any[] }>(`api/tenants/${tenantId}/servers`),
			create: (tenantId: string, data: { name: string; description?: string; serverSoftwareId: string; nodeId: string; volumes?: any[]; network?: any; limits?: any; env?: Record<string, string>; startup?: any }) => request<{ id: string; containerId: string; name: string; state: string; ports: any[] }>(`api/tenants/${tenantId}/servers`, { method: 'POST', body: JSON.stringify(data) }),
			delete: (tenantId: string, id: string) => request<{ ok: boolean }>(`api/tenants/${tenantId}/servers/${id}`, { method: 'DELETE' }),
			websocket: (tenantId: string, id: string) => request<{ token: string; socket: string }>(`api/tenants/${tenantId}/servers/${id}/websocket`),
			logs: (tenantId: string, id: string, tail?: string) => request<{ logs: string }>(`api/tenants/${tenantId}/servers/${id}/logs${tail ? `?tail=${tail}` : ''}`),
			// Power actions
			start: (tenantId: string, id: string) => request<{ ok: boolean }>(`api/tenants/${tenantId}/servers/${id}/start`, { method: 'POST' }),
			stop: (tenantId: string, id: string) => request<{ ok: boolean }>(`api/tenants/${tenantId}/servers/${id}/stop`, { method: 'POST' }),
			restart: (tenantId: string, id: string) => request<{ ok: boolean }>(`api/tenants/${tenantId}/servers/${id}/restart`, { method: 'POST' }),
			kill: (tenantId: string, id: string) => request<{ ok: boolean }>(`api/tenants/${tenantId}/servers/${id}/kill`, { method: 'POST' }),
		},
		billing: {
			getTenantBilling: (tenantId: string) => request<{ tenantId: string; billing: any; balance: any }>(`api/tenants/${tenantId}/billing`),
			getTransactions: (tenantId: string) => request<{ items: any[] }>(`api/tenants/${tenantId}/billing/transactions`),
			addFunds: (tenantId: string, amount: number) => request<{ balance: number; currency: string }>(`api/tenants/${tenantId}/billing/add-funds`, { method: 'POST', body: JSON.stringify({ amount }) }),
			getConfig: () => request<any>('api/billing/config'),
		},
		wallet: {
			get: () => request<{ ruBalance: number; usedResourceUnits: number }>('api/wallet'),
			getTransactions: (page = 1, pageSize = 20) => request<{ items: any[]; meta: any }>(`api/wallet/transactions?page=${page}&pageSize=${pageSize}`),
			addRU: (amount: number) => request<{ ruBalance: number; added: number }>('api/wallet/ru/add', { method: 'POST', body: JSON.stringify({ amount }) }),
			deductRU: (amount: number, reason?: string) => request<{ ruBalance: number; deducted: number }>('api/wallet/ru/deduct', { method: 'POST', body: JSON.stringify({ amount, reason }) }),
			getRUEstimate: (nodeId: string, params?: { cpu?: string; memory?: string; disk?: string }) => {
				const query = new URLSearchParams()
				if (params?.cpu) query.set('cpu', params.cpu)
				if (params?.memory) query.set('memory', params.memory)
				if (params?.disk) query.set('disk', params.disk)
				const qs = query.toString()
				return request<{ ruPerHour: number; ruPerDay: number; ruPerMonth: number; pricePerHour: number; pricePerDay: number; pricePerMonth: number; limits: any }>(`api/wallet/ru/estimate/${nodeId}${qs ? '?' + qs : ''}`)
			},
		},
		admin: {
			users: () => request<{ items: any[] }>('api/admin/users'),
			setAdmin: (id: string, isAdmin: boolean) => request<{ ok: boolean }>(`api/admin/users/${id}/admin`, { method: 'POST', body: JSON.stringify({ isAdmin }) }),
			giveResourceUnits: (userId: string, amount: number, reason?: string) => request<{ userId: string; amount: number; newBalance: number }>('api/admin/ru/give', { method: 'POST', body: JSON.stringify({ user_id: userId, amount, reason }) }),
			setResourceUnits: (userId: string, balance: number, reason?: string) => request<{ userId: string; oldBalance: number; newBalance: number; difference: number }>('api/admin/ru/set', { method: 'POST', body: JSON.stringify({ user_id: userId, balance, reason }) }),
			getUserWallet: (userId: string) => request<{ userId: string; ruBalance: number; usedResourceUnits: number }>(`api/admin/users/${userId}/wallet`),
			tenants: () => request<{ items: any[] }>('api/admin/tenants'),
			updateTenantExtras: (id: string, body: { extraMemoryMb?: number; extraDiskMb?: number; extraCpuPercent?: number; extraServerSlots?: number }) =>
				request<{ ok: boolean }>(`api/admin/tenants/${id}/extras`, { method: 'POST', body: JSON.stringify(body) }),
			packagesGet: () => request<{ defaultPackage: string; items: Record<string, any> }>('api/admin/packages'),
			packagesSet: (body: { items?: Record<string, any>; defaultPackage?: string }) => request<{ ok: boolean }>('api/admin/packages', { method: 'POST', body: JSON.stringify(body) }),
			diagnostics: () => request<{ pterodactyl: { url: string }; redis: { ok: boolean }; db: { ok: boolean } }>('api/admin/diagnostics'),
			eggsSync: () => request<{ ok: boolean; count: number }>('api/admin/jobs/eggs/sync', { method: 'POST' }),
			eggs: () => request<{ items: Array<{ eggId: number; name: string; dockerImage: string }> }>('api/admin/eggs'),
			reconcile: () => request<{ ok: boolean; drifts: string[] }>('api/admin/jobs/reconcile/run', { method: 'POST' }),
			resyncServers: () => request<{ ok: boolean; importedCount: number; imported: Array<{ id: string; pteroServerId: number }> }>('api/admin/jobs/servers/resync', { method: 'POST' }),
			nodes: () => request<{ items: any[] }>('api/admin/nodes'),
			createNode: (data: any) => request<{ ok: boolean; id: string }>('api/admin/nodes', { method: 'POST', body: JSON.stringify(data) }),
			getNode: (id: string) => request<{ node: any }>(`api/admin/nodes/${id}`),
			updateNode: (id: string, data: any) => request<{ ok: boolean }>(`api/admin/nodes/${id}`, { method: 'PATCH', body: JSON.stringify(data) }),
			deleteNode: (id: string) => request<{ ok: boolean }>(`api/admin/nodes/${id}`, { method: 'DELETE' }),
			software: () => request<{ items: any[] }>('api/admin/software'),
			createSoftware: (data: any) => request<{ ok: boolean; id: string }>('api/admin/software', { method: 'POST', body: JSON.stringify(data) }),
			updateSoftware: (id: string, data: any) => request<{ ok: boolean }>(`api/admin/software/${id}`, { method: 'PATCH', body: JSON.stringify(data) }),
			deleteSoftware: (id: string) => request<{ ok: boolean }>(`api/admin/software/${id}`, { method: 'DELETE' }),
		},
	}
}

export function useApi(getToken: () => string | null) {
	const { config } = useApp()
	const normalized = config.apiBaseUrl === '/' ? '/' : config.apiBaseUrl.replace(/\/$/, '/')
	return useMemo(() => createApi(normalized, getToken), [normalized, getToken])
}
