import React, { useCallback, useEffect, useMemo, useState } from 'react'
import { useAuth } from '@/providers/AuthProvider'
import { useApi } from '@/api/client'
import { useTenants } from '@/providers/TenantProvider'
import { useAlert } from '@/components/ui/Alert'
import Spinner from '@/components/ui/Spinner'
import { Button } from '@/components/ui/Button'
import { Select } from '@/components/ui/Select'

interface TenantAuditLog {
    id: string
    action: string
    actorType: string
    createdAt: string
    metadata?: Record<string, unknown> | null
    ip?: string | null
    userAgent?: string | null
    actorEmail?: string | null
    actorUsername?: string | null
    userId?: string | null
    tenantId?: string | null
    serverId?: string | null
    containerId?: string | null
    name?: string | null
}

// Retro bracket frame component
function BracketFrame({ children, className = '' }: { children: React.ReactNode; className?: string }) {
    return (
        <div className={`relative ${className}`}>
            {/* Corner brackets */}
            <span className="absolute -top-1 -left-1 text-neutral-500 dark:text-neutral-600 text-lg font-bold select-none">[</span>
            <span className="absolute -top-1 -right-1 text-neutral-500 dark:text-neutral-600 text-lg font-bold select-none">]</span>
            <span className="absolute -bottom-1 -left-1 text-neutral-500 dark:text-neutral-600 text-lg font-bold select-none">[</span>
            <span className="absolute -bottom-1 -right-1 text-neutral-500 dark:text-neutral-600 text-lg font-bold select-none">]</span>
            <div className="px-4 py-3">
                {children}
            </div>
        </div>
    )
}

// Stat display with LCD styling
function StatBox({ label, value, unit }: { label: string; value: string | number; unit?: string }) {
    return (
        <BracketFrame className="bg-neutral-100 dark:bg-neutral-900/50 border border-neutral-300 dark:border-neutral-800/50">
            <div className="text-[10px] uppercase tracking-[0.2em] text-neutral-600 dark:text-neutral-500 mb-1" style={{ fontFamily: "'Space Mono', monospace" }}>
                {label}
            </div>
            <div className="flex items-baseline gap-1">
                <span className="text-2xl text-red-600 dark:text-red-500" style={{ fontFamily: "'LCD14Condensed', monospace" }}>
                    {value}
                </span>
                {unit && (
                    <span className="text-xs text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                        {unit}
                    </span>
                )}
            </div>
        </BracketFrame>
    )
}

// Resource bar with gradient
function ResourceBar({ label, used, total, unit }: { label: string; used: number; total: number; unit: string }) {
    const percent = total > 0 ? Math.round((used / total) * 100) : 0
    const barSegments = 20
    const filledSegments = Math.round((percent / 100) * barSegments)
    
    return (
        <div className="space-y-2">
            <div className="flex items-center justify-between">
                <span className="text-[10px] uppercase tracking-[0.2em] text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                    {label}
                </span>
                <span className="text-xs text-neutral-700 dark:text-neutral-400" style={{ fontFamily: "'Space Mono', monospace" }}>
                    {used}{unit} / {total}{unit}
                </span>
            </div>
            <div className="flex gap-[2px]">
                {Array.from({ length: barSegments }).map((_, i) => {
                    const isFilled = i < filledSegments
                    // Gradient from yellow to orange to red
                    const segmentPercent = (i / barSegments) * 100
                    let color = 'bg-neutral-300 dark:bg-neutral-800'
                    if (isFilled) {
                        if (segmentPercent < 33) color = 'bg-yellow-500'
                        else if (segmentPercent < 66) color = 'bg-orange-500'
                        else color = 'bg-red-500'
                    }
                    return (
                        <div
                            key={i}
                            className={`h-4 flex-1 ${color} ${isFilled ? 'opacity-100' : 'opacity-30'}`}
                        />
                    )
                })}
            </div>
            <div className="text-right">
                <span className="text-lg text-red-600 dark:text-red-400"  style={{ fontFamily: "'Seven Segment', sans-serif" }}>
                    {percent}%
                </span>
            </div>
        </div>
    )
}

export function DashboardPage() {
    const { token } = useAuth()
    const getTokenFn = useCallback(() => token, [token])
    const api = useApi(getTokenFn)
    const { tenants, selectedTenantId } = useTenants()
    const [resources, setResources] = useState<any | null>(null)
    const [tenantMembers, setTenantMembers] = useState<Array<{ userId: string; email?: string | null; username?: string | null }>>([])
    const [tenantAudit, setTenantAudit] = useState<TenantAuditLog[]>([])
    const [auditTotal, setAuditTotal] = useState(0)
    const [auditPage, setAuditPage] = useState(1)
    const [auditPageSize, setAuditPageSize] = useState(10)
    const [auditLoading, setAuditLoading] = useState(false)
    const { notify } = useAlert()

    const selectedTenant = useMemo(() => tenants.find((tenant) => tenant.id === selectedTenantId) || null, [tenants, selectedTenantId])
    const tenantName = selectedTenant?.name || 'your tenant'

    const totalPages = Math.max(1, Math.ceil(auditTotal / auditPageSize) || 1)
    const hasPagination = auditTotal > auditPageSize

    const formatTimestamp = useCallback((timestamp: string | Date) => {
        const date = typeof timestamp === 'string' ? new Date(timestamp) : timestamp
        if (Number.isNaN(date.getTime())) return String(timestamp)
        const diffMs = Date.now() - date.getTime()
        const diffSeconds = Math.floor(diffMs / 1000)
        if (diffSeconds < 45) return 'just now'
        if (diffSeconds < 90) return '1 min ago'
        const diffMinutes = Math.floor(diffSeconds / 60)
        if (diffMinutes < 60) return `${diffMinutes} min${diffMinutes === 1 ? '' : 's'} ago`
        const diffHours = Math.floor(diffMinutes / 60)
        if (diffHours < 24) return `${diffHours} hour${diffHours === 1 ? '' : 's'} ago`
        const diffDays = Math.floor(diffHours / 24)
        if (diffDays < 7) return `${diffDays} day${diffDays === 1 ? '' : 's'} ago`
        return date.toLocaleString()
    }, [])

    const formatActionLabel = useCallback((action: string) => {
        const actionMap: Record<string, string> = {
            'auth:success': 'Signed in',
            'auth:fail': 'Sign-in failed',
            'auth:logout': 'Signed out',
            'auth:register': 'Account created',
            'account:update': 'Account updated',
            'session:revoke': 'Session revoked',
            'session:issue': 'Session issued',
        }
        if (actionMap[action]) return actionMap[action]
        return action.replace(':', ' · ')
    }, [])

    const summarizeMetadata = useCallback((metadata?: Record<string, unknown> | null) => {
        if (!metadata) return null
        const entries = Object.entries(metadata)
        if (!entries.length) return null
        return entries
            .map(([key, value]) => `${key}: ${typeof value === 'object' ? JSON.stringify(value) : String(value)}`)
            .join(' · ')
    }, [])

    const loadTenantAudit = useCallback(async (page: number, abortRef?: { current: boolean }) => {
        if (!selectedTenantId) return
        setAuditLoading(true)
        try {
            const response = await api.tenants.audit(selectedTenantId, page, auditPageSize)
            if (abortRef?.current) return
            setTenantAudit(response.items || [])
            setAuditTotal(response.meta?.total || 0)
            setAuditPage(page)
        } catch (e) {
            if (!abortRef?.current) {
                console.error('Failed to load tenant audit log:', e)
            }
        } finally {
            if (!abortRef?.current) setAuditLoading(false)
        }
    }, [api, selectedTenantId, auditPageSize])

    const handleAuditPageChange = useCallback((nextPage: number) => {
        if (!selectedTenantId) return
        if (nextPage < 1 || nextPage === auditPage) return
        if (nextPage > totalPages) return
        loadTenantAudit(nextPage)
    }, [auditPage, loadTenantAudit, selectedTenantId, totalPages])

    const handleAuditPageSizeChange = useCallback((value: string) => {
        const size = parseInt(value, 10)
        if (!Number.isFinite(size) || size <= 0) return
        setAuditPageSize(size)
        setAuditPage(1)
        loadTenantAudit(1)
    }, [loadTenantAudit])

    useEffect(() => {
        if (!selectedTenantId) return
        let mounted = true
        const auditAbort = { current: false }
        api.tenants.resources(selectedTenantId)
            .then((r) => mounted && setResources(r))
            .catch((e) => { if (mounted) { notify({ type: 'error', title: 'Failed to load resources', description: e?.message || '' }) } })
        api.tenants.members(selectedTenantId)
            .then((r) => mounted && setTenantMembers(r.items || []))
            .catch(() => mounted && setTenantMembers([]))
        loadTenantAudit(1, auditAbort)
        return () => {
            mounted = false
            auditAbort.current = true
        }
    }, [api, loadTenantAudit, selectedTenantId])

    useEffect(() => {
        setAuditPage(1)
        setTenantAudit([])
    }, [selectedTenantId])

    const usage = useMemo(() => {
        if (!resources) return null
        const pkg = resources.package || {}
        const extra = resources.extra || {}
        const used = resources.used || {}
        const totals = {
            memoryMb: (pkg.memoryMb || 0) + (extra.memoryMb || 0),
            diskMb: (pkg.diskMb || 0) + (extra.diskMb || 0),
            cpuPercent: (pkg.cpuPercent || 0) + (extra.cpuPercent || 0),
            serverSlots: (pkg.serverSlots || 0) + (extra.serverSlots || 0),
        }
        
        return {
            memoryUsed: used.memoryMb || 0,
            memoryTotal: totals.memoryMb,
            diskUsed: used.diskMb || 0,
            diskTotal: totals.diskMb,
            cpuUsed: used.cpuPercent || 0,
            cpuTotal: totals.cpuPercent,
            serversUsed: (used as any).servers || 0,
            serversTotal: totals.serverSlots,
        }
    }, [resources])

    const displayedAudit = useMemo(() => tenantAudit, [tenantAudit])

    return (
        <div className="space-y-8">
            {/* Header 
            <div className="flex items-center gap-4">
                <div className="flex items-center gap-2">
                    <span className="text-neutral-600 dark:text-neutral-500">[</span>
                    <div className="flex gap-1">
                        <span className="w-2 h-2 bg-red-500 rounded-sm"></span>
                        <span className="w-2 h-2 bg-orange-500 rounded-sm"></span>
                        <span className="w-2 h-2 bg-yellow-500 rounded-sm"></span>
                    </div>
                    <span className="text-neutral-600 dark:text-neutral-500">]</span>
                </div>
                <h1 
                    className="text-4xl md:text-5xl text-neutral-900 dark:text-neutral-100 tracking-wider"
                    style={{ fontFamily: "'Seven Segment', sans-serif" }}
                                                
                >
                    DASHBOARD
                </h1>
            </div>
            */}
            {selectedTenantId && (
                <>
                    {/* Stats Grid 
                    <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                        <StatBox label="Memory" value={usage?.memoryUsed || 0} unit="MB" />
                        <StatBox label="Disk" value={usage?.diskUsed || 0} unit="MB" />
                        <StatBox label="CPU" value={usage?.cpuUsed || 0} unit="%" />
                        <StatBox label="Servers" value={usage?.serversUsed || 0} />
                    </div>

                    {/* Resource Overview Section */}
                    <div className="bg-neutral-100 dark:bg-black border border-neutral-300 dark:border-neutral-800/50 p-6">
                        <div className="flex items-center gap-3 mb-6">
                            <h2 
                                className="text-2xl text-neutral-900 dark:text-neutral-100 tracking-wider"
                                 style={{ fontFamily: "'Seven Segment', sans-serif" }}
                            >
                                RESOURCE OVERVIEW
                            </h2>
                            <div className="flex-1 h-px bg-neutral-300 dark:bg-neutral-800"></div>
                        </div>

                        <div className="grid md:grid-cols-2 gap-8">
                            {/* Left: Resource Bars */}
                            <div className="space-y-6">
                                {!resources ? (
                                    Array.from({ length: 4 }).map((_, i) => (
                                        <div key={i} className="space-y-2 animate-pulse">
                                            <div className="flex items-center justify-between">
                                                <div className="h-3 w-20 bg-neutral-800 rounded"></div>
                                                <div className="h-3 w-24 bg-neutral-800 rounded"></div>
                                            </div>
                                            <div className="flex gap-[2px]">
                                                {Array.from({ length: 20 }).map((_, j) => (
                                                    <div key={j} className="h-4 flex-1 bg-neutral-800/50" />
                                                ))}
                                            </div>
                                            <div className="flex justify-end">
                                                <div className="h-6 w-12 bg-neutral-800 rounded"></div>
                                            </div>
                                        </div>
                                    ))
                                ) : (
                                    <>
                                        <ResourceBar 
                                            label="Memory" 
                                            used={usage?.memoryUsed || 0} 
                                            total={usage?.memoryTotal || 1} 
                                            unit="MB" 
                                        />
                                        <ResourceBar 
                                            label="Disk" 
                                            used={usage?.diskUsed || 0} 
                                            total={usage?.diskTotal || 1} 
                                            unit="MB" 
                                        />
                                        <ResourceBar 
                                            label="CPU" 
                                            used={usage?.cpuUsed || 0} 
                                            total={usage?.cpuTotal || 1} 
                                            unit="%" 
                                        />
                                        <ResourceBar 
                                            label="Servers" 
                                            used={usage?.serversUsed || 0} 
                                            total={usage?.serversTotal || 1} 
                                            unit="" 
                                        />
                                    </>
                                )}
                            </div>

                            {/* Right: Tenant Info */}
                            <div className="bg-neutral-50 dark:bg-neutral-900/50 border border-neutral-300 dark:border-neutral-800/50 p-5">
                                <div className="text-[10px] uppercase tracking-[0.2em] text-neutral-600 dark:text-neutral-500 mb-3" style={{ fontFamily: "'Space Mono', monospace" }}>
                                    Active Tenant
                                </div>
                                {!resources ? (
                                    <div className="animate-pulse space-y-4">
                                        <div className="h-10 w-3/4 bg-neutral-200 dark:bg-neutral-800 rounded"></div>
                                        <div className="space-y-2">
                                            <div className="h-3 w-full bg-neutral-200 dark:bg-neutral-800 rounded"></div>
                                            <div className="h-3 w-full bg-neutral-200 dark:bg-neutral-800 rounded"></div>
                                            <div className="h-3 w-2/3 bg-neutral-200 dark:bg-neutral-800 rounded"></div>
                                        </div>
                                        <div className="flex -space-x-2">
                                            {Array.from({ length: 4 }).map((_, i) => (
                                                <div key={i} className="w-14 h-14 rounded border-2 border-white dark:border-neutral-900 bg-neutral-200 dark:bg-neutral-800"></div>
                                            ))}
                                        </div>
                                    </div>
                                ) : (
                                    <>
                                        <div 
                                            className="text-4xl text-neutral-900 dark:text-neutral-100 mb-4"
                                             style={{ fontFamily: "'Seven Segment', sans-serif" }}
                                        >
                                            {tenantName.toUpperCase()}
                                        </div>
                                        <div className="text-xs text-neutral-600 dark:text-neutral-500 leading-relaxed" style={{ fontFamily: "'Space Mono', monospace" }}>
                                            Tenants are where your servers, resources and coins live. You can add others to your tenant to allow them to manage everything alongside you. A tenant is essentially your team or organization within Torch. You can organize multiple tenants for different projects or groups.
                                        </div>
                                        
                                        {/* Member avatars */}
                                        <div className="mt-4 flex items-center gap-2">
                                            <div className="flex -space-x-2">
                                                {Array.from({ length: 4 }).map((_, index) => {
                                                    const member = tenantMembers[index]
                                                    if (member) {
                                                        const colors = ['bg-red-500', 'bg-orange-500', 'bg-yellow-500', 'bg-green-500']
                                                        const initial = member.username?.charAt(0).toUpperCase() || member.email?.charAt(0).toUpperCase() || '?'
                                                        return (
                                                            <div 
                                                                key={member.userId} 
                                                                className={`w-14 h-14 z-50 rounded border-2 border-white dark:border-neutral-900 flex items-center justify-center text-2xl font-bold text-black shadow-sm ${colors[index % colors.length]}`}
                                                            >
                                                                {initial}
                                                            </div>
                                                        )
                                                    } else {
                                                        return (
                                                            <div 
                                                                key={`placeholder-${index}`}
                                                                className="w-14 z-10 h-14 rounded border-2 border-white dark:border-neutral-900 bg-neutral-200 dark:bg-neutral-800/30 flex items-center justify-center text-lg font-bold text-neutral-400 dark:text-white/50 shadow-sm"
                                                            >
                                                                --
                                                            </div>
                                                        )
                                                    }
                                                })}
                                            </div>
                                            {tenantMembers.length > 4 && (
                                                <span className="text-[10px] text-neutral-600 dark:text-neutral-500 uppercase tracking-wider" style={{ fontFamily: "'Space Mono', monospace" }}>
                                                    +{tenantMembers.length - 4} more
                                                </span>
                                            )}
                                        </div>
                                    </>
                                )}
                            </div>
                        </div>
                    </div>

                    {/* Activity Log */}
                    <div className="bg-neutral-100 dark:bg-black border border-neutral-300 dark:border-neutral-800/50 p-6">
                        <div className="flex items-center justify-between gap-4 mb-6">
                            <div className="flex items-center gap-3">
                                <h2 
                                    className="text-2xl text-neutral-900 dark:text-neutral-100 tracking-wider"
                                    style={{ fontFamily: "'Seven Segment', sans-serif" }}
                                >
                                    ACTIVITY LOG
                                </h2>
                                {/*<div className="w-2 h-2 bg-red-500 rounded-full animate-pulse"></div>*/}
                                <div className="flex-1 h-px bg-neutral-300 dark:bg-neutral-800"></div>
                            </div>
                            <div className="flex items-center gap-3 text-xs text-neutral-600 dark:text-neutral-500">
                                <div className="flex items-center gap-2">
                                    <span className="whitespace-nowrap" style={{ fontFamily: "'Space Mono', monospace" }}>Rows</span>
                                    <div className="w-20">
                                        <Select
                                            value={String(auditPageSize)}
                                            onChange={handleAuditPageSizeChange}
                                            options={[
                                                { value: '3', label: '3' },
                                                { value: '10', label: '10' },
                                                { value: '25', label: '25' },
                                                { value: '50', label: '50' },
                                            ]}
                                        />
                                    </div>
                                </div>
                                {hasPagination && (
                                    <div className="flex items-center gap-2" style={{ fontFamily: "'Space Mono', monospace" }}>
                                        <Button
                                            variant="ghost"
                                            size="sm"
                                            className="h-7 w-7 p-0 text-neutral-600 dark:text-neutral-500 hover:text-neutral-900 dark:hover:text-neutral-300"
                                            onClick={() => handleAuditPageChange(auditPage - 1)}
                                            disabled={auditPage === 1 || auditLoading}
                                        >
                                            ‹
                                        </Button>
                                        <span className="text-neutral-700 dark:text-neutral-400">
                                            {auditPage}/{totalPages}
                                        </span>
                                        <Button
                                            variant="ghost"
                                            size="sm"
                                            className="h-7 w-7 p-0 text-neutral-600 dark:text-neutral-500 hover:text-neutral-900 dark:hover:text-neutral-300"
                                            onClick={() => handleAuditPageChange(auditPage + 1)}
                                            disabled={auditPage >= totalPages || auditLoading}
                                        >
                                            ›
                                        </Button>
                                    </div>
                                )}
                            </div>
                        </div>

                        <div className="bg-neutral-50 dark:bg-neutral-900/50 border border-neutral-300 dark:border-neutral-800/50 overflow-hidden">
                            {auditLoading && tenantAudit.length === 0 ? (
                                <div className="p-8 flex items-center justify-center">
                                    <Spinner size="lg" />
                                </div>
                            ) : displayedAudit.length === 0 ? (
                                <div className="p-8 text-center">
                                    <p className="text-sm text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                                        No activity yet
                                    </p>
                                </div>
                            ) : (
                                <div className="divide-y divide-neutral-200 dark:divide-neutral-800/50">
                                    {displayedAudit.map((item) => {
                                        return (
                                            <div key={item.id} className="p-4 hover:bg-neutral-100 dark:hover:bg-neutral-800/20 transition-colors">
                                                <div className="flex items-start justify-between gap-4">
                                                    <div className="flex items-start gap-3 flex-1">
                                                        <span className="w-2 h-2 bg-red-500 rounded-full mt-1.5"></span>
                                                        <div className="flex-1 space-y-1">
                                                            <div className="flex items-center gap-2 flex-wrap">
                                                                <span className="text-sm font-medium text-neutral-900 dark:text-neutral-200" style={{ fontFamily: "'Space Mono', monospace" }}>
                                                                    {formatActionLabel(item.action)}
                                                                </span>
                                                                {item.name && (
                                                                    <span className="text-xs text-red-600 dark:text-red-400 bg-red-500/10 px-2 py-0.5 rounded" style={{ fontFamily: "'Space Mono', monospace" }}>
                                                                        {item.name}
                                                                    </span>
                                                                )}
                                                            </div>
                                                            <div className="flex flex-wrap gap-x-4 gap-y-1 text-[10px] text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                                                                {item.actorUsername && (
                                                                    <span>by {item.actorUsername}</span>
                                                                )}
                                                                {item.serverId && (
                                                                    <span>Server: {item.serverId.slice(0, 8)}...</span>
                                                                )}
                                                                {item.containerId && (
                                                                    <span>Container: {item.containerId.slice(0, 8)}...</span>
                                                                )}
                                                                {item.ip && (
                                                                    <span>IP: {item.ip}</span>
                                                                )}
                                                            </div>
                                                        </div>
                                                    </div>
                                                    <span className="text-[10px] text-neutral-500 dark:text-neutral-600 uppercase tracking-wider whitespace-nowrap" style={{ fontFamily: "'Space Mono', monospace" }}>
                                                        {formatTimestamp(item.createdAt)}
                                                    </span>
                                                </div>
                                            </div>
                                        )
                                    })}
                                </div>
                            )}
                        </div>
                    </div>
                </>
            )}
        </div>
    )
}
