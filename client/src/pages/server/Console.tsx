import React, { useCallback, useEffect, useRef, useState } from 'react'
import { useAuth } from '@/providers/AuthProvider'
import { useApi } from '@/api/client'
import { useTenants } from '@/providers/TenantProvider'
import { useParams, useNavigate } from 'react-router-dom'
import { Button } from '@/components/ui/Button'
import { Input } from '@/components/ui/Input'
import Spinner from '@/components/ui/Spinner'
import { useAlert } from '@/components/ui/Alert'
import { ArrowLeftIcon } from '@heroicons/react/24/outline'

interface LogLine {
    id: number
    text: string
    type: 'stdout' | 'info' | 'error' | 'success' | 'status'
    timestamp: Date
}

interface ServerStats {
    memory_bytes: number
    memory_limit_bytes: number
    cpu_absolute: number
    network: { rx_bytes: number; tx_bytes: number }
    state: string
    disk_bytes: number
}

// Resource bar component (same as Dashboard)
function ResourceBar({ label, used, total, unit }: { label: string; used: number; total: number; unit: string }) {
    const percent = total > 0 ? Math.round((used / total) * 100) : 0
    const barSegments = 10
    const filledSegments = Math.round((percent / 100) * barSegments)
    
    return (
        <div className="space-y-1">
            <div className="flex items-center justify-between">
                <span className="text-[10px] uppercase tracking-[0.15em] text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                    {label}
                </span>
                <span className="text-[10px] text-neutral-700 dark:text-neutral-400" style={{ fontFamily: "'Space Mono', monospace" }}>
                    {used.toFixed(0)}{unit}
                </span>
            </div>
            <div className="flex gap-[1px]">
                {Array.from({ length: barSegments }).map((_, i) => {
                    const isFilled = i < filledSegments
                    const segmentPercent = (i / barSegments) * 100
                    let color = 'bg-neutral-300 dark:bg-neutral-800'
                    if (isFilled) {
                        if (segmentPercent < 33) color = 'bg-yellow-500'
                        else if (segmentPercent < 66) color = 'bg-orange-500'
                        else color = 'bg-red-500'
                    }
                    return (
                        <div key={i} className={`h-2 flex-1 ${color} ${isFilled ? 'opacity-100' : 'opacity-30'}`} />
                    )
                })}
            </div>
        </div>
    )
}

export function ConsolePage() {
    const { serverId } = useParams<{ serverId: string }>()
    const { token } = useAuth()
    const getTokenFn = useCallback(() => token, [token])
    const api = useApi(getTokenFn)
    const { selectedTenantId } = useTenants()
    const { notify } = useAlert()
    const navigate = useNavigate()

    const [server, setServer] = useState<any>(null)
    const [loading, setLoading] = useState(true)
    const [error, setError] = useState<string | null>(null)
    const [connected, setConnected] = useState(false)
    const [connecting, setConnecting] = useState(false)
    const [serverStatus, setServerStatus] = useState<string>('offline')
    const [stats, setStats] = useState<ServerStats | null>(null)
    const [logs, setLogs] = useState<LogLine[]>([])
    const [command, setCommand] = useState('')
    const [commandHistory, setCommandHistory] = useState<string[]>([])
    const [historyIndex, setHistoryIndex] = useState(-1)
    const [powerLoading, setPowerLoading] = useState<string | null>(null)

    const wsRef = useRef<WebSocket | null>(null)
    const logContainerRef = useRef<HTMLDivElement>(null)
    const logIdRef = useRef(0)
    const commandInputRef = useRef<HTMLInputElement>(null)

    const addLog = useCallback((text: string, type: LogLine['type'] = 'stdout') => {
        setLogs(prev => {
            const newLog: LogLine = {
                id: logIdRef.current++,
                text,
                type,
                timestamp: new Date()
            }
            const updated = [...prev, newLog]
            return updated.slice(-500)
        })
    }, [])

    const formatTimestamp = useCallback((date: Date) => {
        return date.toLocaleTimeString('en-US', { hour12: false })
    }, [])

    const formatBytes = useCallback((bytes: number) => {
        if (bytes === 0) return 0
        const k = 1024
        const sizes = ['B', 'KB', 'MB', 'GB']
        const i = Math.floor(Math.log(bytes) / Math.log(k))
        return parseFloat((bytes / Math.pow(k, i)).toFixed(1))
    }, [])

    const loadServer = useCallback(async () => {
        if (!selectedTenantId || !serverId) return

        try {
            setLoading(true)
            setError(null)
            const serversRes = await api.servers.list(selectedTenantId)
            const foundServer = serversRes.items.find((s: any) => s.id === serverId)
            
            if (!foundServer) {
                setError('Server not found')
                return
            }
            
            setServer(foundServer)
            setServerStatus(foundServer.state || 'offline')
        } catch (e: any) {
            setError(e?.message || 'Failed to load server')
        } finally {
            setLoading(false)
        }
    }, [api, selectedTenantId, serverId])

    const connectWebSocket = useCallback(async () => {
        if (!server || !selectedTenantId) return

        setConnecting(true)
        addLog('Generating token...', 'info')

        try {
            // Get WebSocket credentials from backend (which proxies to lightd)
            const credentials = await api.servers.websocket(selectedTenantId, server.id)
            
            addLog('Connecting to console...', 'info')
            
            const ws = new WebSocket(credentials.socket)
            wsRef.current = ws

            ws.onopen = () => {
                addLog('WebSocket connected, waiting for logs...', 'success')
            }

            ws.onmessage = (event) => {
                try {
                    const data = JSON.parse(event.data)
                    
                    switch (data.event) {
                        case 'init':
                            // Init message: [container_id, container_uuid, status]
                            if (data.args && data.args.length >= 3) {
                                setConnected(true)
                                setConnecting(false)
                                setServerStatus(data.args[2])
                                addLog(`Connected to container`, 'success')
                            }
                            break
                        case 'console_output':
                            if (data.args && data.args[0]) {
                                addLog(data.args[0], 'stdout')
                            }
                            break
                        case 'status':
                            if (data.args && data.args[0]) {
                                setServerStatus(data.args[0])
                                addLog(`Status: ${data.args[0]}`, 'status')
                            }
                            break
                        case 'stats':
                            try {
                                const statsData = typeof data.args[0] === 'string' 
                                    ? JSON.parse(data.args[0]) 
                                    : data.args[0]
                                setStats(statsData)
                            } catch {}
                            break
                        case 'daemon_message':
                            if (data.args && data.args[0]) {
                                addLog(`[daemon] ${data.args[0]}`, 'info')
                            }
                            break
                        case 'error':
                            if (data.args && data.args[0]) {
                                addLog(`Error: ${data.args[0]}`, 'error')
                            }
                            break
                        default:
                            if (data.args && data.args.length > 0) {
                                addLog(`[${data.event}] ${data.args.join(' ')}`, 'info')
                            }
                    }
                } catch {
                    // Raw text message
                    addLog(event.data, 'stdout')
                }
            }

            ws.onclose = (event) => {
                setConnected(false)
                setConnecting(false)
                addLog(`Disconnected (code: ${event.code})`, 'info')
            }

            ws.onerror = () => {
                setConnected(false)
                setConnecting(false)
                addLog('Connection error', 'error')
            }
        } catch (e: any) {
            setConnecting(false)
            addLog(`Failed to connect: ${e?.message || 'Unknown error'}`, 'error')
        }
    }, [server, selectedTenantId, api, addLog])

    const disconnectWebSocket = useCallback(() => {
        if (wsRef.current) {
            wsRef.current.close()
            wsRef.current = null
        }
        setConnected(false)
    }, [])

    const sendCommand = useCallback((cmd: string) => {
        if (!wsRef.current || !connected || !cmd.trim()) return

        const trimmedCmd = cmd.trim()
        wsRef.current.send(JSON.stringify({ event: 'send_command', args: [trimmedCmd] }))
        addLog(`> ${trimmedCmd}`, 'info')

        setCommandHistory(prev => [trimmedCmd, ...prev.filter(c => c !== trimmedCmd)].slice(0, 50))
        setHistoryIndex(-1)
        setCommand('')
    }, [connected, addLog])

    const sendPowerAction = useCallback(async (action: string) => {
        if (!selectedTenantId || !serverId) return
        
        setPowerLoading(action)
        const actionLabels: Record<string, string> = {
            start: 'Starting', stop: 'Stopping', restart: 'Restarting', kill: 'Force stopping'
        }
        addLog(`${actionLabels[action] || action} server...`, 'info')

        try {
            switch (action) {
                case 'start': await api.servers.start(selectedTenantId, serverId); break
                case 'stop': await api.servers.stop(selectedTenantId, serverId); break
                case 'restart': await api.servers.restart(selectedTenantId, serverId); break
                case 'kill': await api.servers.kill(selectedTenantId, serverId); break
            }
            notify({ description: `${actionLabels[action]} server...`, type: 'success' })
        } catch (e: any) {
            addLog(`Failed to ${action}: ${e?.message}`, 'error')
            notify({ description: e?.message || `Failed to ${action}`, type: 'error' })
        } finally {
            setPowerLoading(null)
        }
    }, [api, selectedTenantId, serverId, notify, addLog])

    const handleCommandSubmit = useCallback((e: React.FormEvent) => {
        e.preventDefault()
        sendCommand(command)
    }, [command, sendCommand])

    const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
        if (e.key === 'ArrowUp') {
            e.preventDefault()
            if (historyIndex < commandHistory.length - 1) {
                const newIndex = historyIndex + 1
                setHistoryIndex(newIndex)
                setCommand(commandHistory[newIndex])
            }
        } else if (e.key === 'ArrowDown') {
            e.preventDefault()
            if (historyIndex > 0) {
                setHistoryIndex(historyIndex - 1)
                setCommand(commandHistory[historyIndex - 1])
            } else {
                setHistoryIndex(-1)
                setCommand('')
            }
        }
    }, [historyIndex, commandHistory])

    // Auto-scroll logs
    useEffect(() => {
        if (logContainerRef.current) {
            logContainerRef.current.scrollTop = logContainerRef.current.scrollHeight
        }
    }, [logs])

    useEffect(() => { loadServer() }, [loadServer])
    
    // Connect WebSocket when server is loaded
    useEffect(() => {
        if (server && !connected && !connecting) {
            connectWebSocket()
        }
        return () => disconnectWebSocket()
    }, [server])

    const isRunning = serverStatus === 'running' || serverStatus === 'ready'
    const isOffline = serverStatus === 'offline' || serverStatus === 'stopped' || serverStatus === 'exited'
    const isStopping = serverStatus === 'stopping'
    const isStarting = serverStatus === 'starting' || serverStatus === 'installing'

    if (!selectedTenantId) {
        return (
            <div className="bg-neutral-100 dark:bg-black border border-neutral-300 dark:border-neutral-800/50 p-6">
                <p className="text-sm text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                    Select a tenant first
                </p>
            </div>
        )
    }

    if (loading) {
        return <div className="flex items-center justify-center py-12"><Spinner size="lg" /></div>
    }

    if (error || !server) {
        return (
            <div className="bg-neutral-100 dark:bg-black border border-neutral-300 dark:border-neutral-800/50 p-6">
                <div className="text-red-600 dark:text-red-400" style={{ fontFamily: "'Space Mono', monospace" }}>
                    {error || 'Server not found'}
                </div>
            </div>
        )
    }

    return (
        <div className="space-y-4 md:space-y-6">
            {/* Header - Mobile Responsive */}
            <div className="flex flex-col sm:flex-row sm:items-center gap-3">
                <div className="flex items-center gap-3">
                    <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => navigate('/servers')}
                        className="text-neutral-600 dark:text-neutral-400 hover:text-neutral-900 dark:hover:text-neutral-100 p-1"
                    >
                        <ArrowLeftIcon className="w-4 h-4" />
                    </Button>
                    <div className="flex gap-1">
                        <span className={`w-2 h-2 rounded-sm ${isRunning ? 'bg-green-500' : isOffline ? 'bg-red-500' : 'bg-yellow-500'}`}></span>
                        <span className={`w-2 h-2 rounded-sm ${connected ? 'bg-green-500' : 'bg-neutral-500'}`}></span>
                    </div>
                    <h1 className="text-xl md:text-2xl text-neutral-900 dark:text-neutral-100 tracking-wider truncate" style={{ fontFamily: "'Seven Segment', sans-serif" }}>
                        {server.name.toUpperCase()}
                    </h1>
                </div>
                <div className="flex items-center gap-2 sm:ml-auto">
                    <div className="flex-1 h-px bg-neutral-300 dark:bg-neutral-800 sm:hidden"></div>
                    <span className={`text-[10px] px-2 py-1 rounded whitespace-nowrap ${
                        isRunning ? 'bg-green-500/20 text-green-600 dark:text-green-400' :
                        isOffline ? 'bg-red-500/20 text-red-600 dark:text-red-400' :
                        'bg-yellow-500/20 text-yellow-600 dark:text-yellow-400'
                    }`} style={{ fontFamily: "'Space Mono', monospace" }}>
                        {serverStatus.toUpperCase()}
                    </span>
                </div>
            </div>

            {/* Power Actions & Stats - Mobile Responsive */}
            <div className="bg-neutral-100 dark:bg-black border border-neutral-300 dark:border-neutral-800/50 p-3 md:p-4">
                <div className="flex flex-col gap-4">
                    {/* Power Buttons - Wrap on mobile */}
                    <div className="flex flex-wrap items-center gap-2">
                        <span className="text-[10px] uppercase tracking-[0.15em] text-neutral-600 dark:text-neutral-500 mr-1" style={{ fontFamily: "'Space Mono', monospace" }}>
                            Power
                        </span>
                        <button
                            onClick={() => sendPowerAction('start')}
                            disabled={!!powerLoading || isRunning || isStarting}
                            className="px-2 md:px-3 py-1 text-[10px] md:text-xs bg-green-600 hover:bg-green-700 disabled:bg-neutral-700 disabled:text-neutral-500 text-white transition-colors"
                            style={{ fontFamily: "'Space Mono', monospace" }}
                        >
                            {powerLoading === 'start' ? '...' : 'START'}
                        </button>
                        <button
                            onClick={() => sendPowerAction('restart')}
                            disabled={!!powerLoading || isOffline}
                            className="px-2 md:px-3 py-1 text-[10px] md:text-xs bg-orange-600 hover:bg-orange-700 disabled:bg-neutral-700 disabled:text-neutral-500 text-white transition-colors"
                            style={{ fontFamily: "'Space Mono', monospace" }}
                        >
                            {powerLoading === 'restart' ? '...' : 'RESTART'}
                        </button>
                        <button
                            onClick={() => sendPowerAction('stop')}
                            disabled={!!powerLoading || isOffline}
                            className="px-2 md:px-3 py-1 text-[10px] md:text-xs bg-red-600 hover:bg-red-700 disabled:bg-neutral-700 disabled:text-neutral-500 text-white transition-colors"
                            style={{ fontFamily: "'Space Mono', monospace" }}
                        >
                            {powerLoading === 'stop' ? '...' : 'STOP'}
                        </button>
                        <button
                            onClick={() => sendPowerAction('kill')}
                            disabled={!!powerLoading || isOffline}
                            className="px-2 md:px-3 py-1 text-[10px] md:text-xs bg-red-800 hover:bg-red-900 disabled:bg-neutral-700 disabled:text-neutral-500 text-white transition-colors"
                            style={{ fontFamily: "'Space Mono', monospace" }}
                        >
                            {powerLoading === 'kill' ? '...' : 'KILL'}
                        </button>
                    </div>

                    {/* Stats - Grid on mobile */}
                    <div className="grid grid-cols-3 gap-2 md:gap-4">
                        <ResourceBar
                            label="MEM"
                            used={stats ? formatBytes(stats.memory_bytes) : 0}
                            total={server.memoryMb || 512}
                            unit="MB"
                        />
                        <ResourceBar
                            label="CPU"
                            used={stats?.cpu_absolute || 0}
                            total={server.cpuPercent || 100}
                            unit="%"
                        />
                        <ResourceBar
                            label="DISK"
                            used={stats ? formatBytes(stats.disk_bytes) : 0}
                            total={(server.diskMb || 5) * 1024}
                            unit="MB"
                        />
                    </div>
                </div>
            </div>

            {/* Console */}
            <div className="bg-neutral-100 dark:bg-black border border-neutral-300 dark:border-neutral-800/50 p-3 md:p-4">
                <div className="flex items-center gap-2 md:gap-3 mb-3 md:mb-4">
                    <h2 className="text-base md:text-lg text-neutral-900 dark:text-neutral-100 tracking-wider" style={{ fontFamily: "'Seven Segment', sans-serif" }}>
                        CONSOLE
                    </h2>
                    <div className="flex-1 h-px bg-neutral-300 dark:bg-neutral-800"></div>
                    <div className="flex items-center gap-2">
                        {connecting ? (
                            <span className="text-[10px] text-yellow-600 dark:text-yellow-400" style={{ fontFamily: "'Space Mono', monospace" }}>
                                CONNECTING...
                            </span>
                        ) : connected ? (
                            <span className="text-[10px] text-green-600 dark:text-green-400" style={{ fontFamily: "'Space Mono', monospace" }}>
                                ● LIVE
                            </span>
                        ) : (
                            <button
                                onClick={connectWebSocket}
                                className="text-[10px] text-red-600 dark:text-red-400 hover:text-red-500" 
                                style={{ fontFamily: "'Space Mono', monospace" }}
                            >
                                ○ RECONNECT
                            </button>
                        )}
                    </div>
                </div>

                {/* Log output - Responsive height */}
                <div
                    ref={logContainerRef}
                    className="bg-black border border-neutral-800 p-2 md:p-4 h-64 md:h-96 overflow-y-auto font-mono text-xs md:text-sm"
                    onClick={() => commandInputRef.current?.focus()}
                >
                    {logs.length === 0 ? (
                        <div className="text-neutral-600" style={{ fontFamily: "'Space Mono', monospace" }}>
                            {connecting ? 'Connecting...' : 'Waiting for output...'}
                        </div>
                    ) : (
                        logs.map((log) => (
                            <div
                                key={log.id}
                                className={`leading-relaxed break-all ${
                                    log.type === 'error' ? 'text-red-400' :
                                    log.type === 'success' ? 'text-green-400' :
                                    log.type === 'info' ? 'text-blue-400' :
                                    log.type === 'status' ? 'text-yellow-400' :
                                    'text-neutral-300'
                                }`}
                                style={{ fontFamily: "'Space Mono', monospace" }}
                            >
                                <span className="text-neutral-600 mr-1 md:mr-2 hidden sm:inline">[{formatTimestamp(log.timestamp)}]</span>
                                {log.text}
                            </div>
                        ))
                    )}
                </div>

                {/* Command input */}
                <form onSubmit={handleCommandSubmit} className="mt-2 flex gap-2">
                    <div className="flex-1 relative">
                        <span className="absolute left-2 md:left-3 top-1/2 -translate-y-1/2 text-neutral-500 text-xs md:text-sm" style={{ fontFamily: "'Space Mono', monospace" }}>
                            &gt;
                        </span>
                        <Input
                            ref={commandInputRef}
                            value={command}
                            onChange={(e) => setCommand(e.target.value)}
                            onKeyDown={handleKeyDown}
                            placeholder={connected ? "Command..." : "Connect first"}
                            disabled={!connected}
                            className="pl-6 md:pl-8 text-xs md:text-sm bg-black border-neutral-800 text-neutral-100 placeholder:text-neutral-600"
                            style={{ fontFamily: "'Space Mono', monospace" }}
                        />
                    </div>
                    <Button
                        type="submit"
                        disabled={!connected || !command.trim()}
                        className="px-3 md:px-4 text-xs md:text-sm bg-red-600 hover:bg-red-700 disabled:bg-neutral-800 text-white"
                        style={{ fontFamily: "'Space Mono', monospace" }}
                    >
                        SEND
                    </Button>
                </form>
            </div>

            {/* Server Info - Mobile Responsive */}
            <div className="bg-neutral-100 dark:bg-black border border-neutral-300 dark:border-neutral-800/50 p-3 md:p-4">
                <div className="flex items-center gap-2 md:gap-3 mb-3 md:mb-4">
                    <h2 className="text-base md:text-lg text-neutral-900 dark:text-neutral-100 tracking-wider" style={{ fontFamily: "'Seven Segment', sans-serif" }}>
                        INFO
                    </h2>
                    <div className="flex-1 h-px bg-neutral-300 dark:bg-neutral-800"></div>
                </div>
                <div className="grid grid-cols-2 md:grid-cols-4 gap-3 md:gap-4 text-[10px] md:text-xs" style={{ fontFamily: "'Space Mono', monospace" }}>
                    <div>
                        <div className="text-[9px] md:text-[10px] uppercase tracking-[0.15em] text-neutral-600 dark:text-neutral-500 mb-1">Container</div>
                        <div className="text-neutral-900 dark:text-neutral-100 truncate">{server.containerId?.slice(0, 8) || 'N/A'}</div>
                    </div>
                    <div>
                        <div className="text-[9px] md:text-[10px] uppercase tracking-[0.15em] text-neutral-600 dark:text-neutral-500 mb-1">Node</div>
                        <div className="text-neutral-900 dark:text-neutral-100 truncate">{server.node || 'Unknown'}</div>
                    </div>
                    <div>
                        <div className="text-[9px] md:text-[10px] uppercase tracking-[0.15em] text-neutral-600 dark:text-neutral-500 mb-1">Memory</div>
                        <div className="text-neutral-900 dark:text-neutral-100">{server.memoryMb || 0} MB</div>
                    </div>
                    <div>
                        <div className="text-[9px] md:text-[10px] uppercase tracking-[0.15em] text-neutral-600 dark:text-neutral-500 mb-1">Disk</div>
                        <div className="text-neutral-900 dark:text-neutral-100">{server.diskMb || 0} MB</div>
                    </div>
                </div>
            </div>
        </div>
    )
}
