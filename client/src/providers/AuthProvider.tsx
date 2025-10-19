import React, { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react'
import { useApi } from '@/api/client'

interface User {
	id: string
	email: string
	username: string | null
	discordId: string | null
	isAdmin?: boolean
	createdAt: string
	updatedAt: string
}

interface AuthContextValue {
	token: string | null
	user: User | null
	isAuthenticated: boolean
	login: (identifier: string, password: string) => Promise<void>
	register: (email: string, username: string, password: string) => Promise<void>
	logout: () => Promise<void>
	refreshUser: () => Promise<void>
}

const AuthContext = createContext<AuthContextValue | undefined>(undefined)

function getStoredToken(): string | null {
	try { return localStorage.getItem('manhattan_token') } catch { return null }
}

export const AuthProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
	const [token, setToken] = useState<string | null>(getStoredToken())
	const [user, setUser] = useState<User | null>(null)
	const getTokenFn = useCallback(() => token, [token])
	const api = useApi(getTokenFn)

	const persistToken = useCallback((t: string | null) => {
		setToken(t)
		try { if (t) localStorage.setItem('manhattan_token', t); else localStorage.removeItem('manhattan_token') } catch {}
	}, [])

	const refreshUser = useCallback(async () => {
		if (!token) { setUser(null); return }
		try {
			const data = await api.auth.me()
			setUser(data.user)
		} catch {
			setUser(null)
		}
	}, [api, token])

	// Refresh only when token changes
	useEffect(() => {
		let ignore = false
		async function run() {
			if (!token) { setUser(null); return }
			try {
				const data = await api.auth.me()
				if (!ignore) setUser(data.user)
			} catch {
				if (!ignore) setUser(null)
			}
		}
		run()
		return () => { ignore = true }
	}, [token, api])

	const login = useCallback(async (identifier: string, password: string) => {
		const res = await api.auth.login({ identifier, password })
		persistToken(res.token)
		await refreshUser()
	}, [api, persistToken, refreshUser])

	const register = useCallback(async (email: string, username: string, password: string) => {
		const res = await api.auth.register({ email, username, password })
		persistToken(res.token)
		await refreshUser()
	}, [api, persistToken, refreshUser])

	const logout = useCallback(async () => {
		try { await api.auth.logout() } catch {}
		persistToken(null)
		setUser(null)
	}, [api, persistToken])

	const value = useMemo<AuthContextValue>(() => ({ token, user, isAuthenticated: !!token && !!user, login, register, logout, refreshUser }), [token, user, login, register, logout, refreshUser])
	return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>
}

export function useAuth() {
	const ctx = useContext(AuthContext)
	if (!ctx) throw new Error('useAuth must be used within AuthProvider')
	return ctx
}

