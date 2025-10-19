import React, { useCallback, useEffect, useMemo, useState } from 'react'
import { useAuth } from '@/providers/AuthProvider'
import { useApi } from '@/api/client'
import { useTenants } from '@/providers/TenantProvider'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card'
import { useAlert } from '@/components/ui/Alert'
import Spinner from '@/components/ui/Spinner'
import { Button } from '@/components/ui/Button'
import { Select } from '@/components/ui/Select'
import { ChartPieIcon } from '@heroicons/react/24/solid'
import { CubeTransparentIcon } from '@heroicons/react/24/outline'

const MEMBER_COLORS = ['bg-yellow-500', 'bg-blue-500', 'bg-green-500', 'bg-purple-500']
const MEMBER_TEXT_COLORS = ['text-yellow-800', 'text-blue-800', 'text-green-800', 'text-purple-800']
const PLACEHOLDER_COLOR = 'bg-neutral-400 dark:bg-neutral-600'

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

    const visibleMembers = useMemo(() => {
        const maxSlots = 4
        return tenantMembers.slice(0, Math.min(tenantMembers.length, maxSlots))
    }, [tenantMembers])

    const placeholderCount = useMemo(() => {
        const maxSlots = 4
        return Math.max(0, maxSlots - visibleMembers.length)
    }, [visibleMembers.length])

    const extraMemberCount = Math.max(0, tenantMembers.length - 4)

    const displayedAudit = useMemo(() => tenantAudit, [tenantAudit])

    const totalPages = Math.max(1, Math.ceil(auditTotal / auditPageSize) || 1)
    const hasPagination = auditTotal > auditPageSize

    // No expansion/overlay; page size controls pagination directly

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
        
        const memoryUsedMb = used.memoryMb || 0
        const diskUsedMb = used.diskMb || 0
        const cpuUsedPercent = used.cpuPercent || 0
        const serversUsed = (used as any).servers || 0
        
        const memoryUtilization = totals.memoryMb > 0 ? Math.round((memoryUsedMb / totals.memoryMb) * 100) : 0
        const diskUtilization = totals.diskMb > 0 ? Math.round((diskUsedMb / totals.diskMb) * 100) : 0
        const cpuUtilization = totals.cpuPercent > 0 ? Math.round((cpuUsedPercent / totals.cpuPercent) * 100) : 0
        const serverUtilization = totals.serverSlots > 0 ? Math.round((serversUsed / totals.serverSlots) * 100) : 0
        
        return {
            memory: `${memoryUsedMb}MB / ${totals.memoryMb}MB`,
            memoryPercent: memoryUtilization,
            disk: `${diskUsedMb}MB / ${totals.diskMb}MB`,
            diskPercent: diskUtilization,
            cpu: `${cpuUsedPercent}% / ${totals.cpuPercent}%`,
            cpuPercent: cpuUtilization,
            servers: `${serversUsed} / ${totals.serverSlots}`,
            serversPercent: serverUtilization,
        }
    }, [resources])

    return (
        <div className="space-y-6">
            <div>
                <h1 className="text-2xl font-semibold text-neutral-900 dark:text-neutral-100">Dashboard</h1>
            </div>

            {selectedTenantId && (
                <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_minmax(320px,360px)]">
                    <div className="bg-neutral-200/50 dark:bg-neutral-800/50 rounded-lg">
                        <div className="px-2 pt-2 pb-1 flex items-center gap-2">
                            <ChartPieIcon className="h-4 w-4 text-neutral-600/30 dark:text-neutral-400/30" />
                            <span className="text-xs text-neutral-700 dark:text-neutral-300 font-semibold">Resources</span>
                        </div>
                        <div className="grid grid-cols-2 p-1 gap-1 h-80 items-stretch">
                            <Card className="flex h-full flex-col p-3">
                                <div className="text-sm font-semibold text-neutral-900 dark:text-neutral-100 mb-3">Memory</div>
                                <div className="flex flex-col gap-2 flex-1 justify-center">
                                    <div className="text-sm text-neutral-900 mt-10 dark:text-neutral-100">
                                        <span className="font-semibold">{usage?.memory?.split(' / ')[0]}</span>
                                        <span className="text-neutral-500 dark:text-neutral-400"> / </span>
                                        <span>{usage?.memory?.split(' / ')[1]}</span>
                                    </div>
                                    <div className="text-xs text-neutral-600 dark:text-neutral-400">
                                        {usage?.memoryPercent}% utilized
                                    </div>
                                </div>
                            </Card>
                            <Card className="flex h-full flex-col p-3">
                                <div className="text-sm font-semibold text-neutral-900 dark:text-neutral-100 mb-3">Disk</div>
                                <div className="flex flex-col gap-2 flex-1 justify-center">
                                    <div className="text-sm text-neutral-900 mt-10 dark:text-neutral-100">
                                        <span className="font-semibold">{usage?.disk?.split(' / ')[0]}</span>
                                        <span className="text-neutral-500 dark:text-neutral-400"> / </span>
                                        <span>{usage?.disk?.split(' / ')[1]}</span>
                                    </div>
                                    <div className="text-xs text-neutral-600 dark:text-neutral-400">
                                        {usage?.diskPercent}% utilized
                                    </div>
                                </div>
                            </Card>
                            <Card className="flex h-full flex-col p-3">
                                <div className="text-sm font-semibold text-neutral-900 dark:text-neutral-100 mb-3">CPU</div>
                                <div className="flex flex-col gap-2 flex-1 justify-center">
                                    <div className="text-sm text-neutral-900 mt-10 dark:text-neutral-100">
                                        <span className="font-semibold">{usage?.cpu?.split(' / ')[0]}</span>
                                        <span className="text-neutral-500 dark:text-neutral-400"> / </span>
                                        <span>{usage?.cpu?.split(' / ')[1]}</span>
                                    </div>
                                    <div className="text-xs text-neutral-600 dark:text-neutral-400">
                                        {usage?.cpuPercent}% utilized
                                    </div>
                                </div>
                            </Card>
                            <Card className="flex h-full flex-col p-3">
                                <div className="text-sm font-semibold text-neutral-900 dark:text-neutral-100 mb-3">Servers</div>
                                <div className="flex flex-col gap-2 flex-1 justify-center">
                                    <div className="text-sm text-neutral-900 mt-10 dark:text-neutral-100">
                                        <span className="font-semibold">{usage?.servers?.split(' / ')[0]}</span>
                                        <span className="text-neutral-500 dark:text-neutral-400"> / </span>
                                        <span>{usage?.servers?.split(' / ')[1]}</span>
                                    </div>
                                    <div className="text-xs text-neutral-600 dark:text-neutral-400">
                                        {usage?.serversPercent}% utilized
                                    </div>
                                </div>
                            </Card>
                        </div>
                    </div>

                    {selectedTenant && (
                        <Card className="backdrop-blur bg-white/70 dark:bg-neutral-800/30 border-neutral-200/60 dark:border-neutral-700/60 flex flex-col h-full">
                            <CardHeader className="pb-2">
                                {/* Tenant Icon */}
                                <div className="relative">
                                    <div className="bg-white dark:from-neutral-900 dark:to-neutral-900 shadow-xs rounded-lg border border-neutral-200 dark:border-neutral-700/60 h-10 w-10 flex items-center justify-center">
                                        <CubeTransparentIcon className="h-5 w-5 text-neutral-600 dark:text-neutral-400" />
                                    </div>
                                </div>
                                <CardTitle className="text-base mt-6 text-neutral-900 dark:text-neutral-100">You're managing <span className="text-neutral-950 dark:text-white">{tenantName}</span></CardTitle>
                                <p className="text-sm mt-2 text-neutral-600 dark:text-neutral-400 leading-relaxed">
                                    Tenants are where your servers, resources and coins live. You can add others to your tenant to allow them to manage everything alongside you.
                                </p>
                            </CardHeader>
                            <CardContent className="pt-0 flex flex-col flex-1">
                                <div className="mt-auto pt-4 flex items-center gap-3">
                                    <div className="flex -space-x-3">
                                        {visibleMembers.map((member, index) => {
                                            const colorClass = MEMBER_COLORS[index % MEMBER_COLORS.length]
                                            const textColorClass = MEMBER_TEXT_COLORS[index % MEMBER_TEXT_COLORS.length]
                                            const initial = member.username?.charAt(0).toUpperCase() || member.email?.charAt(0).toUpperCase() || 'U'
                                            return (
                                                <div key={member.userId} className={`flex h-10 w-10 items-center justify-center rounded-lg border-2 border-white dark:border-neutral-900 text-xs font-medium uppercase shadow-sm ${colorClass} ${textColorClass}`}>
                                                    {initial}
                                                </div>
                                            )
                                        })}
                                        {placeholderCount > 0 && new Array(placeholderCount).fill(null).map((_, index) => (
                                            <div key={`placeholder-${index}`} className={`flex h-10 w-10 items-center justify-center rounded-lg border-2 border-white dark:border-neutral-900 text-xs font-medium uppercase text-white/50 shadow-sm ${PLACEHOLDER_COLOR}`}>
                                                --
                                            </div>
                                        ))}
                                    </div>
                                    {extraMemberCount > 0 && (
                                        <span className="text-xs font-semibold uppercase tracking-[0.3em] text-neutral-700 dark:text-neutral-300">
                                            + {extraMemberCount} more
                                        </span>
                                    )}
                                </div>
                            </CardContent>
                        </Card>
                    )}
                </div>
            )}

			{selectedTenantId && (
				<div className="space-y-4">
					<div className="flex items-start justify-between gap-4">
						<div>
							<h2 className="text-sm font-semibold text-neutral-900 dark:text-neutral-100">Recent activity</h2>
							<p className="text-xs text-neutral-500 dark:text-neutral-400 mt-1">An activity log of recent actions</p>
						</div>
						<div className="flex items-center gap-3 text-xs text-neutral-500 dark:text-neutral-400">
							<div className="flex items-center gap-2">
								<span className="whitespace-nowrap">Rows</span>
								<div className="w-24">
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
								<>
									<span className="hidden sm:inline">
										{Math.min((auditPage - 1) * auditPageSize + 1, auditTotal)}–{Math.min(auditPage * auditPageSize, auditTotal)} of {auditTotal}
									</span>
									<div className="flex items-center gap-2">
										<Button
											variant="ghost"
											size="sm"
											className="h-7 w-7 rounded-full p-0"
											onClick={() => handleAuditPageChange(auditPage - 1)}
											disabled={auditPage === 1 || auditLoading}
											aria-label="Previous audit page"
										>
											<span className="sr-only">Previous</span>
											‹
										</Button>
										<span className="font-semibold text-neutral-700 dark:text-neutral-300">
											{auditPage} / {totalPages}
										</span>
										<Button
											variant="ghost"
											size="sm"
											className="h-7 w-7 rounded-full p-0"
											onClick={() => handleAuditPageChange(auditPage + 1)}
											disabled={auditPage >= totalPages || auditLoading}
											aria-label="Next audit page"
										>
											<span className="sr-only">Next</span>
											›
										</Button>
									</div>
								</>
							)}
						</div>
					</div>
					<div>
						{auditLoading && tenantAudit.length === 0 ? (
							<div className="relative">
								<div className="absolute left-4 top-0 bottom-0 w-px bg-gradient-to-b from-neutral-300 via-indigo-700 to-neutral-300 dark:from-neutral-700 dark:via-indigo-400 dark:to-neutral-700" />
								<div className="space-y-3">
								{(() => { const skLen = 5; return Array.from({ length: skLen }).map((_, idx) => (
										<div key={idx} className="timeline-item relative flex gap-3 pl-10">
											<div className="absolute left-4 top-1/2 h-2 w-2 -translate-x-1/2 -translate-y-1/2 rounded-sm bg-neutral-400 dark:bg-neutral-500 animate-pulse" />
										<div className="flex-1 rounded-md border border-neutral-200/80 dark:border-neutral-700/60 bg-white/60 dark:bg-neutral-900/40 p-3">
											<div className="h-3 w-40 bg-neutral-200 dark:bg-neutral-700 rounded animate-pulse" />
											<div className="mt-2 h-2.5 w-60 bg-neutral-200 dark:bg-neutral-700 rounded animate-pulse" />
											<div className="mt-3 h-2 w-24 bg-neutral-200 dark:bg-neutral-700 rounded animate-pulse" />
										</div>
									</div>
								)) })()}
								</div>
							</div>
						) : displayedAudit.length === 0 ? (
							<div className="rounded-md border border-dashed border-neutral-200 dark:border-neutral-700 py-8 text-center">
								<p className="text-sm font-medium text-neutral-600 dark:text-neutral-300">No activity yet</p>
								<p className="text-xs text-neutral-500 dark:text-neutral-400 mt-1">Actions from you and other members will show up here.</p>
							</div>
						) : (
							<div className="relative">
								<div className="space-y-3">
									{displayedAudit.map((item, idx) => {
										const formattedMeta = summarizeMetadata(item.metadata)
										const isFirst = idx === 0
										const isLast = idx === displayedAudit.length - 1
										return (
											<div key={item.id} className="timeline-item relative flex gap-3">
												<div className="flex-1 rounded-md border border-neutral-200/80 dark:border-neutral-700/60 bg-white/60 dark:bg-neutral-900/40 py-2 px-3 pt-3">
													<div className="flex items-center justify-between gap-3">
														<div className="flex items-center gap-2 text-sm text-neutral-900 dark:text-neutral-100 font-medium">
															<span>{formatActionLabel(item.action)}</span>
															{item.actorUsername && (
																<span className="flex text-xs mt-0.5 text-neutral-500 dark:text-neutral-400">by {item.actorUsername}
                                                                <div className="flex items-center gap-2">
                                                                    <div className="w-4 h-4 text-[10px] rounded-md ml-1 pl-1.25 pt-0.5 bg-neutral-200 dark:bg-neutral-700">
                                                                        {item.actorUsername?.charAt(0).toUpperCase()}
                                                                    </div>
                                                                </div>
                                                                </span>
															)}
														</div>
														<span className="text-xs text-neutral-500 dark:text-neutral-400">{formatTimestamp(item.createdAt)}</span>
													</div>
													{formattedMeta && (
														<pre style={{ fontFamily: 'Space Mono, sans-serif' }} className="mt-1 text-xs text-neutral-500 w-98 dark:text-neutral-400 bg-white shadow-xs border border-neutral-200/80 dark:border-transparent dark:bg-neutral-800/50 px-2 py-1 rounded-md mt-2">{formattedMeta}</pre>
													)}
													<div className="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-[10px] uppercase tracking-[0.3em] text-neutral-500 dark:text-neutral-400">
														{item.ip && <span>IP {item.ip}</span>}
														{item.userAgent && <span>{item.userAgent}</span>}
													</div>
												</div>
											</div>
										)
									})}
								</div>

								{hasPagination && (
									<div className="mt-4 flex items-center justify-center gap-2 text-xs text-neutral-500 dark:text-neutral-400">
										<Button
											variant="ghost"
											size="sm"
											className="h-7 w-7 rounded-full p-0"
											onClick={() => handleAuditPageChange(auditPage - 1)}
											disabled={auditPage === 1 || auditLoading}
											aria-label="Previous audit page"
										>
											<span className="sr-only">Previous</span>
											‹
										</Button>
										<span className="font-semibold text-neutral-700 dark:text-neutral-300">
											{auditPage} / {totalPages}
										</span>
										<Button
											variant="ghost"
											size="sm"
											className="h-7 w-7 rounded-full p-0"
											onClick={() => handleAuditPageChange(auditPage + 1)}
											disabled={auditPage >= totalPages || auditLoading}
											aria-label="Next audit page"
										>
											<span className="sr-only">Next</span>
											›
										</Button>
									</div>
								)}

							</div>
						)}
					</div>
				</div>
			)}
        </div>
    )
}

