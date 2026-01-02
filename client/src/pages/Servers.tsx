import React, { useCallback, useEffect, useState, useRef } from 'react'
import { useAuth } from '@/providers/AuthProvider'
import { useNavigate, Link } from 'react-router-dom'
import { useApi } from '@/api/client'
import { useTenants } from '@/providers/TenantProvider'
import { useTenantUpdates } from '@/providers/MQTTProvider'
import { Button } from '@/components/ui/Button'
import { Modal } from '@/components/ui/Modal'
import Spinner from '@/components/ui/Spinner'
import { TrashIcon, CommandLineIcon, PlusIcon } from '@heroicons/react/24/outline'
import { useAlert } from '@/components/ui/Alert'
import gsap from 'gsap'

// Strip ANSI escape codes from log output
function stripAnsi(text: string): string {
    return text.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, '')
}

interface ServerCardProps {
    server: any
    onDelete: (server: any) => void
    onNavigate: (id: string) => void
    tenantId: string
    api: any
}

function ServerCard({ server, onDelete, onNavigate, tenantId, api }: ServerCardProps) {
    const cardRef = useRef<HTMLDivElement>(null)
    const logsRef = useRef<HTMLDivElement>(null)
    const [isHovered, setIsHovered] = useState(false)
    const [logs, setLogs] = useState<string[]>([])
    const [loadingLogs, setLoadingLogs] = useState(false)

    const fetchLogs = useCallback(async () => {
        if (!server.containerId) return
        setLoadingLogs(true)
        try {
            const res = await api.servers.logs(tenantId, server.id, '10')
            if (res.logs) {
                const logLines = res.logs.split('\n').filter((line: string) => line.trim())
                setLogs(logLines.length > 0 ? logLines : ['No logs available'])
            } else {
                setLogs(['No logs available'])
            }
        } catch {
            setLogs([`Unable to fetch logs for ${server.name}`])
        } finally {
            setLoadingLogs(false)
        }
    }, [api, tenantId, server])

    const handleMouseEnter = () => {
        setIsHovered(true)
        fetchLogs()

        if (logsRef.current) {
            // Kill any existing animations
            gsap.killTweensOf(logsRef.current)
            
            // Set initial state and animate
            gsap.set(logsRef.current, { display: 'block', height: 'auto' })
            const height = logsRef.current.offsetHeight
            gsap.fromTo(logsRef.current, 
                { height: 0, opacity: 0 },
                { 
                    height: height, 
                    opacity: 1, 
                    duration: 0.25, 
                    ease: 'power2.out',
                    force3D: true
                }
            )
        }
    }

    const handleMouseLeave = () => {
        setIsHovered(false)

        if (logsRef.current) {
            // Kill any existing animations
            gsap.killTweensOf(logsRef.current)
            
            gsap.to(logsRef.current, {
                opacity: 0,
                height: 0,
                duration: 0.2,
                ease: 'power2.in',
                force3D: true,
                onComplete: () => {
                    if (logsRef.current) {
                        gsap.set(logsRef.current, { display: 'none' })
                    }
                }
            })
        }
    }

    return (
        <div
            ref={cardRef}
            onMouseEnter={handleMouseEnter}
            onMouseLeave={handleMouseLeave}
            className={`p-4 transition-colors cursor-pointer ${
                isHovered 
                    ? 'bg-neutral-200/50 dark:bg-neutral-800/40' 
                    : 'hover:bg-neutral-100 dark:hover:bg-neutral-800/20'
            }`}
        >
            <div className="flex items-start justify-between gap-4">
                <div className="flex items-start gap-3 flex-1">
                    <span className={`w-2 h-2 rounded-full mt-1.5 ${
                        server.state === 'running' || server.state === 'ready'
                            ? 'bg-green-500'
                            : server.state === 'stopped' || server.state === 'exited'
                            ? 'bg-red-500'
                            : 'bg-yellow-500'
                    }`}></span>
                    <div className="flex-1 space-y-2">
                        <div className="flex items-center gap-2 flex-wrap">
                            <span className="text-sm font-medium text-neutral-900 dark:text-neutral-200" style={{ fontFamily: "'Space Mono', monospace" }}>
                                {server.name}
                            </span>
                            <span className={`text-xs px-2 py-0.5 rounded ${
                                server.state === 'running' || server.state === 'ready'
                                    ? 'bg-green-500/10 text-green-600 dark:text-green-400'
                                    : server.state === 'stopped' || server.state === 'exited'
                                    ? 'bg-red-500/10 text-red-600 dark:text-red-400'
                                    : 'bg-yellow-500/10 text-yellow-600 dark:text-yellow-400'
                            }`} style={{ fontFamily: "'Space Mono', monospace" }}>
                                {server.state || 'unknown'}
                            </span>
                        </div>
                        <div className="flex flex-wrap gap-x-4 gap-y-1 text-[10px] text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                            <span>Image: {server.dockerImage}</span>
                            {server.containerId && (
                                <span>ID: {server.containerId.substring(0, 12)}</span>
                            )}
                            {server.limits?.memory && (
                                <span>RAM: {server.limits.memory}</span>
                            )}
                            {server.limits?.disk && (
                                <span>Disk: {server.limits.disk}</span>
                            )}
                            {server.limits?.cpu && (
                                <span>CPU: {server.limits.cpu}</span>
                            )}
                            {server.ports && server.ports.length > 0 && (
                                <span>Ports: {server.ports.map((p: any) => p.hostPort).join(', ')}</span>
                            )}
                        </div>
                        {server.description && (
                            <div className="text-xs text-neutral-500 dark:text-neutral-600" style={{ fontFamily: "'Space Mono', monospace" }}>
                                {server.description}
                            </div>
                        )}
                    </div>
                </div>
                <div className="flex items-center gap-2">
                    <Button
                        variant="ghost"
                        size="sm"
                        onClick={(e) => { e.stopPropagation(); onNavigate(server.id) }}
                        className="text-blue-600 dark:text-blue-400 hover:text-blue-700 dark:hover:text-blue-300 h-8 w-8 p-0"
                        title="Console"
                    >
                        <CommandLineIcon className="w-4 h-4" />
                    </Button>
                    <Button
                        variant="ghost"
                        size="sm"
                        onClick={(e) => { e.stopPropagation(); onDelete(server) }}
                        className="text-red-600 dark:text-red-400 hover:text-red-700 dark:hover:text-red-300 h-8 w-8 p-0"
                        title="Delete"
                    >
                        <TrashIcon className="w-4 h-4" />
                    </Button>
                </div>
            </div>

            {/* Logs Preview - Hidden by default, shown on hover */}
            <div
                ref={logsRef}
                className="overflow-hidden"
                style={{ display: 'none', opacity: 0, height: 0 }}
            >
                <div className="mt-4 pt-4 border-t border-neutral-200 dark:border-neutral-800/50">
                    <div className="flex items-center gap-2 mb-2">
                        <span className="text-[10px] uppercase tracking-[0.2em] text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                            RECENT LOGS
                        </span>
                        {loadingLogs && <Spinner size="sm" />}
                    </div>
                    <div className="bg-neutral-900 dark:bg-black p-3 max-h-32 overflow-y-auto font-mono text-xs">
                        {logs.length === 0 ? (
                            <span className="text-neutral-500">No logs available</span>
                        ) : (
                            logs.map((log, i) => (
                                <div key={i} className="text-green-400 dark:text-green-500 leading-relaxed">
                                    {stripAnsi(log)}
                                </div>
                            ))
                        )}
                    </div>
                </div>
            </div>
        </div>
    )
}

export function ServersPage() {
    const { token } = useAuth()
    const getTokenFn = useCallback(() => token, [token])
    const api = useApi(getTokenFn)
    const { selectedTenantId } = useTenants()
    const { notify } = useAlert()
    const navigate = useNavigate()
    
    // Live updates via MQTT
    const liveUpdates = useTenantUpdates(selectedTenantId)
    
    const [servers, setServers] = useState<any[]>([])
    const [error, setError] = useState<string | null>(null)
    const [loading, setLoading] = useState(false)
    const [showDeleteModal, setShowDeleteModal] = useState(false)
    const [serverToDelete, setServerToDelete] = useState<any>(null)
    const [deleting, setDeleting] = useState(false)

    const loadData = useCallback(async () => {
        if (!selectedTenantId) return
        setLoading(true)
        setError(null)
        
        try {
            const serversRes = await api.servers.list(selectedTenantId)
            setServers(serversRes.items || [])
        } catch (e: any) {
            setError(e?.message || 'Failed to load data')
        } finally {
            setLoading(false)
        }
    }, [api, selectedTenantId])

    useEffect(() => {
        loadData()
    }, [loadData])

    // Handle live updates
    useEffect(() => {
        if (!liveUpdates.length) return
        
        const latestUpdate = liveUpdates[liveUpdates.length - 1]
        
        switch (latestUpdate.type) {
            case 'server_created':
                notify({ 
                    type: 'success', 
                    description: `Server "${latestUpdate.data.name}" was created`
                })
                loadData()
                break
                
            case 'server_deleted':
                notify({ 
                    type: 'info', 
                    description: `A server was deleted`
                })
                loadData()
                break
                
            case 'member_added':
            case 'member_removed':
                notify({ 
                    type: 'info', 
                    description: `Team membership was updated`
                })
                break
        }
    }, [liveUpdates, notify, loadData])

    const handleDeleteClick = (server: any) => {
        setServerToDelete(server)
        setShowDeleteModal(true)
    }

    const handleDeleteConfirm = async () => {
        if (!selectedTenantId || !serverToDelete) return
        
        setDeleting(true)
        try {
            await api.servers.delete(selectedTenantId, serverToDelete.id)
            notify({ type: 'success', description: 'Server deleted successfully' })
            setShowDeleteModal(false)
            setServerToDelete(null)
            loadData()
        } catch (e: any) {
            notify({ type: 'error', description: e?.message || 'Failed to delete server' })
        } finally {
            setDeleting(false)
        }
    }

    if (!selectedTenantId) return (
        <div className="bg-neutral-100 dark:bg-neutral-900/30 border border-neutral-300 dark:border-neutral-800/50 p-6">
            <p className="text-sm text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                Select a tenant to view servers
            </p>
        </div>
    )

    if (loading) return (
        <div className="flex items-center justify-center py-12"><Spinner size="lg" /></div>
    )

    if (error) return (
        <div className="bg-neutral-100 dark:bg-neutral-900/30 border border-neutral-300 dark:border-neutral-800/50 p-6">
            <p className="text-sm text-red-600 dark:text-red-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                {error}
            </p>
        </div>
    )

    return (
        <div className="space-y-8">
            {/* Servers Section */}
            <div className="bg-neutral-100 dark:bg-black border border-neutral-300 dark:border-neutral-800/50 p-6">
                <div className="flex items-center gap-3 mb-6">
                    <h2 
                        className="text-2xl text-neutral-900 dark:text-neutral-100 tracking-wider"
                        style={{ fontFamily: "'Seven Segment', sans-serif" }}
                    >
                        SERVERS
                    </h2>
                    <div className="flex-1 h-px bg-neutral-300 dark:bg-neutral-800"></div>
                    {/*
                    Created a deploy thing in sidebar - nadhi.dev
                    <Link to="/servers/create">
                        <Button
                            size="sm"
                            className="bg-red-600 hover:bg-red-700 text-white border-transparent"
                            style={{ fontFamily: "'Space Mono', monospace" }}
                        >
                            <PlusIcon className="w-4 h-4 mr-1" />
                            CREATE
                        </Button>
                    </Link>*/}
                </div>

                <div className="bg-neutral-50 dark:bg-neutral-900/50 border border-neutral-300 dark:border-neutral-800/50 overflow-hidden">
                    {servers.length === 0 ? (
                        <div className="p-8 text-center">
                            <p className="text-sm text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                                No servers found
                            </p>
                            <p className="text-xs text-neutral-500 dark:text-neutral-600 mt-1" style={{ fontFamily: "'Space Mono', monospace" }}>
                                Create your first server to get started
                            </p>
                        </div>
                    ) : (
                        <div className="divide-y divide-neutral-200 dark:divide-neutral-800/50">
                            {servers.map((server) => (
                                <ServerCard
                                    key={server.id}
                                    server={server}
                                    onDelete={handleDeleteClick}
                                    onNavigate={(id) => navigate(`/server/${id}`)}
                                    tenantId={selectedTenantId}
                                    api={api}
                                />
                            ))}
                        </div>
                    )}
                </div>
            </div>

            {/* Delete Confirmation Modal */}
            <Modal 
                open={showDeleteModal} 
                onClose={() => setShowDeleteModal(false)}
                title="Delete Server"
            >
                <div className="space-y-4">
                    <p className="text-sm text-neutral-600 dark:text-neutral-400">
                        Are you sure you want to delete <strong>{serverToDelete?.name}</strong>? This action cannot be undone.
                    </p>
                    
                    <div className="flex justify-end gap-2 pt-4">
                        <Button
                            type="button"
                            variant="ghost"
                            onClick={() => setShowDeleteModal(false)}
                            disabled={deleting}
                            className="border-0"
                        >
                            Cancel
                        </Button>
                        <Button 
                            onClick={handleDeleteConfirm}
                            isLoading={deleting}
                            className="bg-red-600 hover:bg-red-700 text-white border-transparent"
                        >
                            Delete Server
                        </Button>
                    </div>
                </div>
            </Modal>
        </div>
    )
}
