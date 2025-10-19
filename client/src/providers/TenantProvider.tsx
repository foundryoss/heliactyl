import React, { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react'
import { useApi } from '@/api/client'
import { useAuth } from '@/providers/AuthProvider'

interface TenantSummary { id: string; name: string; packageId?: string; role?: 'owner' | 'user' | string }

interface TenantContextValue {
    tenants: TenantSummary[]
    selectedTenantId: string | null
    setSelectedTenantId: (id: string | null) => void
    refreshTenants: () => Promise<void>
}

const TenantContext = createContext<TenantContextValue | undefined>(undefined)

export const TenantProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
    const { token } = useAuth()
    const getTokenFn = useCallback(() => token, [token])
    const api = useApi(getTokenFn)
    const [tenants, setTenants] = useState<TenantSummary[]>([])
    const [selectedTenantId, setSelectedTenantId] = useState<string | null>(null)

    const refreshTenants = useCallback(async () => {
        try {
            const r = await api.tenants.list()
            const items = (r.items || []) as TenantSummary[]
            setTenants(items)
            setSelectedTenantId((current) => {
                if (current && items.some((t) => t.id === current)) return current
                return items[0]?.id || null
            })
        } catch {
            setTenants([])
        }
    }, [api])

    useEffect(() => {
        if (!token) { setTenants([]); setSelectedTenantId(null); return }
        refreshTenants()
    }, [token])

    const value = useMemo<TenantContextValue>(() => ({ tenants, selectedTenantId, setSelectedTenantId, refreshTenants }), [tenants, selectedTenantId])
    return <TenantContext.Provider value={value}>{children}</TenantContext.Provider>
}

export function useTenants() {
    const ctx = useContext(TenantContext)
    if (!ctx) throw new Error('useTenants must be used within TenantProvider')
    return ctx
}


