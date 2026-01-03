import React, { useCallback, useEffect, useRef, useState } from 'react'
import { useAuth } from '@/providers/AuthProvider'
import { useApi } from '@/api/client'
import { useTenants } from '@/providers/TenantProvider'
import { useParams, useNavigate } from 'react-router-dom'
import { Button } from '@/components/ui/Button'
import { Input } from '@/components/ui/Input'
import Spinner from '@/components/ui/Spinner'
import { PlayIcon, StopIcon, ArrowPathIcon, BoltIcon } from '@heroicons/react/24/outline'
import { AreaChart, Area, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import { ServerNavigation } from '@/components/ServerNavigation'
import { useServerStore } from '@/state/server'
import { useServerWebSocket } from '@/hooks/useServerWebSocket'
import { useServerPower } from '@/hooks/useServerPower'
import { useServer } from '@/hooks/useServer'
import LoadingAnimation from '@/components/loaders'

interface LogLine { id: number; text: string; type: 'stdout' | 'info' | 'error' | 'success' | 'status'; timestamp: Date }

// Resource card with big Seven Segment display
function ResourceCard({ label, value, unit, subValue, color = 'text-red-500' }: { 
    label: string; value: string | number; unit: string; subValue?: string; color?: string 
}) {
    return (
        <div className="bg-neutral-100 dark:bg-black border border-neutral-300 dark:border-neutral-800/50 p-4 md:p-6">
            <div className="text-[10px] uppercase tracking-[0.2em] text-neutral-600 dark:text-neutral-500 mb-2" style={{ fontFamily: "'Space Mono', monospace" }}>
                {label}
            </div>
            <div className="flex items-baseline gap-2">
                <span className={`text-5xl md:text-6xl ${color}`} style={{ fontFamily: "'Seven Segment', sans-serif" }}>
                    {value}
                </span>
                <span className="text-lg text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                    {unit}
                </span>
            </div>
            {subValue && (
                <div className="text-xs text-neutral-600 dark:text-neutral-600 mt-2" style={{ fontFamily: "'Space Mono', monospace" }}>
                    {subValue}
                </div>
            )}
        </div>
    )
}

// Power button - each with its own background card
function PowerButton({ icon: Icon, label, onClick, disabled, loading, variant = 'default' }: { 
    icon: React.ElementType; label: string; onClick: () => void; disabled?: boolean; loading?: boolean; variant?: 'start' | 'stop' | 'kill' | 'default'
}) {
    const variants = {
        start: 'bg-green-600 hover:bg-green-700 disabled:bg-neutral-300 dark:disabled:bg-neutral-800',
        stop: 'bg-red-600 hover:bg-red-700 disabled:bg-neutral-300 dark:disabled:bg-neutral-800',
        kill: 'bg-red-800 hover:bg-red-900 disabled:bg-neutral-300 dark:disabled:bg-neutral-800',
        default: 'bg-orange-600 hover:bg-orange-700 disabled:bg-neutral-300 dark:disabled:bg-neutral-800'
    }
    return (
        <button onClick={onClick} disabled={disabled || loading} className={`flex items-center gap-2 px-4 py-3 text-white text-xs transition-colors disabled:text-neutral-500 border border-neutral-300 dark:border-neutral-800/50 ${variants[variant]}`} style={{ fontFamily: "'Space Mono', monospace" }}>
            {loading ? <Spinner size="sm" /> : <Icon className="w-4 h-4" />}
            {label}
        </button>
    )
}

export function ConsolePage() {
    const { serverId } = useParams<{ serverId: string }>()
    const { token } = useAuth()
    const getTokenFn = useCallback(() => token, [token])
    const api = useApi(getTokenFn)
    const { selectedTenantId } = useTenants()
    const navigate = useNavigate()

    // Local state
    const [logs, setLogs] = useState<LogLine[]>([])
    const [command, setCommand] = useState('')
    const [commandHistory, setCommandHistory] = useState<string[]>([])
    const [historyIndex, setHistoryIndex] = useState(-1)

    const logContainerRef = useRef<HTMLDivElement>(null)
    const logIdRef = useRef(0)
    const commandInputRef = useRef<HTMLInputElement>(null)

    // Fetch server with caching and background updates
    const { server, isLoading: loading, error } = useServer({
        tenantId: selectedTenantId,
        serverId: serverId || null,
        api
    })

    // Global server state from Zustand (for real-time stats)
    const {
        status: serverStatus,
        stats,
        statsHistory,
        isRunning,
        isOffline
    } = useServerStore()

    const addLog = useCallback((text: string, type: LogLine['type'] = 'stdout') => {
        setLogs(prev => [...prev, { id: logIdRef.current++, text, type, timestamp: new Date() }].slice(-500))
    }, [])

    const formatTimestamp = useCallback((date: Date) => date.toLocaleTimeString('en-US', { hour12: false }), [])
    const formatBytes = useCallback((bytes: number): number => bytes === 0 ? 0 : parseFloat((bytes / (1024 * 1024)).toFixed(1)), [])

    // WebSocket hook
    const { connect, sendCommand: wsSendCommand, connected, connecting } = useServerWebSocket({
        onLog: addLog,
        tenantId: selectedTenantId,
        serverId: serverId || null,
        api
    })

    // Power actions hook
    const { sendPowerAction, loading: powerLoading } = useServerPower({
        tenantId: selectedTenantId,
        serverId: serverId || null,
        api,
        onLog: addLog
    })

    // Command handling
    const handleSendCommand = useCallback((cmd: string) => {
        if (!cmd.trim()) return
        
        const trimmedCmd = cmd.trim()
        const success = wsSendCommand(trimmedCmd)
        
        if (success) {
            addLog(`> ${trimmedCmd}`, 'info')
            setCommandHistory(prev => [trimmedCmd, ...prev.filter(c => c !== trimmedCmd)].slice(0, 50))
            setHistoryIndex(-1)
            setCommand('')
        }
    }, [wsSendCommand, addLog])

    const handleCommandSubmit = useCallback((e: React.FormEvent) => { 
        e.preventDefault()
        handleSendCommand(command)
    }, [command, handleSendCommand])
    
    const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
        if (e.key === 'ArrowUp') { 
            e.preventDefault()
            if (historyIndex < commandHistory.length - 1) { 
                const i = historyIndex + 1
                setHistoryIndex(i)
                setCommand(commandHistory[i])
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

    // Connect WebSocket when server is loaded - only trigger once when server becomes available
    const hasConnectedRef = useRef(false)
    useEffect(() => { 
        if (server && serverId && !hasConnectedRef.current) {
            hasConnectedRef.current = true
            connect()
        }
    }, [server?.id, serverId])
    
    // Reset connection ref when serverId changes
    useEffect(() => {
        hasConnectedRef.current = false
        setLogs([])
    }, [serverId])
    
    // Parse limits from server response
    const parseLimit = (limitStr: string): number => {
        if (!limitStr) return 0
        const match = limitStr.match(/^(\d+(?:\.\d+)?)(MB|GB|%)$/)
        if (!match) return 0
        const value = parseFloat(match[1])
        const unit = match[2]
        if (unit === 'GB') return value * 1024
        if (unit === 'MB') return value
        return value
    }
    
    const memoryLimit = server?.limits?.memory ? parseLimit(server.limits.memory) : (server?.memoryMb || 512)
    const diskLimit = server?.limits?.disk ? parseLimit(server.limits.disk) : (server?.diskMb || 5120)
    const cpuLimit = server?.limits?.cpu ? parseFloat(server.limits.cpu) * 100 : (server?.cpuPercent || 100)
    
    // When offline, show zero for all resources
    const memoryMb = (stats && isRunning) ? formatBytes(stats.memory_bytes) : 0
    const memoryPercent = (stats && isRunning && memoryLimit) ? Math.round((memoryMb / memoryLimit) * 100) : 0
    const cpuPercent = (stats && isRunning) ? stats.cpu_absolute : 0
    const diskMb = (stats && isRunning) ? formatBytes(stats.disk_bytes) : 0
    const diskPercent = (isRunning && diskLimit) ? Math.round((diskMb / diskLimit) * 100) : 0

    // Chart data - show zero line if no history
    const chartData = statsHistory.length > 0 ? statsHistory : [{ time: '--:--:--', memory: 0, cpu: 0 }]

    if (!selectedTenantId) return (
        <div className="bg-neutral-100 dark:bg-black border border-neutral-300 dark:border-neutral-800/50 p-6">
            <p className="text-sm text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                Select a tenant first
            </p>
        </div>
    )
    
    if (loading) return (
        <div className="flex items-center justify-center py-12"><LoadingAnimation/></div>
    )
    
    if (error) return (
        <div className="bg-neutral-100 dark:bg-black border border-neutral-300 dark:border-neutral-800/50 p-6">
            <div className="text-red-600 dark:text-red-400" style={{ fontFamily: "'Space Mono', monospace" }}>
                {typeof error === 'string' ? error : (error as any)?.message || 'Failed to load server'}
            </div>
        </div>
    )
    
    if (!server) return null

    return (
        <div className="space-y-4">
            <ServerNavigation activeTab="console" />
           
            {/* Top Row: Server Info (left) + Power Buttons (right) */}
            <div className="bg-neutral-100 flex dark:bg-black border border-neutral-300 dark:border-neutral-800/50 p-6">
                <div className="flex flex-col md:flex-row gap-4 items-end md:items-center justify-end">
                    {/* Server Name + Status */}
                    <div className="flex-1">
                        <div className=" gap-3 mb-2">
                            <h1 className="text-3xl md:text-4xl text-neutral-900 dark:text-neutral-100 tracking-wider" style={{ fontFamily: "'Seven Segment', sans-serif" }}>
                                {server.name.toUpperCase()}
                            </h1>
                        </div>
                        <div className=" gap-3">
                            <span className={`text-xs px-2 py-0.5 rounded ${isRunning ? 'bg-green-500/10 text-green-600 dark:text-green-400' : isOffline ? 'bg-red-500/10 text-red-600 dark:text-red-400' : 'bg-yellow-500/10 text-yellow-600 dark:text-yellow-400'}`} style={{ fontFamily: "'Space Mono', monospace" }}>
                                {isOffline && (serverStatus === 'running' || serverStatus === 'ready') ? 'STOPPED' : (serverStatus || 'UNKNOWN').toUpperCase()}
                            </span>
                            <span className="text-[10px] text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                                {server.containerId?.slice(0, 12) || 'N/A'}
                            </span>
                        </div>
                    </div>

                    {/* Power Buttons */}
                    <div className="flex flex-wrap gap-2">
                        <PowerButton icon={PlayIcon} label="START" onClick={() => sendPowerAction('start')} disabled={!!powerLoading || isRunning} loading={powerLoading === 'start'} variant="start" />
                        <PowerButton icon={ArrowPathIcon} label="RESTART" onClick={() => sendPowerAction('restart')} disabled={!!powerLoading || isOffline} loading={powerLoading === 'restart'} variant="default" />
                        <PowerButton icon={StopIcon} label="STOP" onClick={() => sendPowerAction('stop')} disabled={!!powerLoading || isOffline} loading={powerLoading === 'stop'} variant="stop" />
                        <PowerButton icon={BoltIcon} label="KILL" onClick={() => sendPowerAction('kill')} disabled={!!powerLoading || isOffline} loading={powerLoading === 'kill'} variant="kill" />
                    </div>
                </div>
            </div>

            {/* Resource Cards Row */}
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                <ResourceCard 
                    label="CPU Usage" 
                    value={cpuPercent.toFixed(1)} 
                    unit="%" 
                    subValue={`of ${cpuLimit.toFixed(0)}% limit`}
                    color={isOffline ? 'text-neutral-500 dark:text-neutral-600' : cpuPercent > 80 ? 'text-red-500' : cpuPercent > 50 ? 'text-orange-500' : 'text-yellow-500'} 
                />
                <ResourceCard 
                    label="Memory" 
                    value={memoryPercent} 
                    unit="%" 
                    subValue={`${memoryMb.toFixed(0)} / ${memoryLimit.toFixed(0)} MB`}
                    color={isOffline ? 'text-neutral-500 dark:text-neutral-600' : memoryPercent > 80 ? 'text-red-500' : memoryPercent > 50 ? 'text-orange-500' : 'text-yellow-500'} 
                />
                <ResourceCard 
                    label="Disk" 
                    value={diskPercent} 
                    unit="%" 
                    subValue={`${diskMb.toFixed(0)} / ${diskLimit.toFixed(0)} MB`}
                    color={isOffline ? 'text-neutral-500 dark:text-neutral-600' : diskPercent > 80 ? 'text-red-500' : diskPercent > 50 ? 'text-orange-500' : 'text-yellow-500'} 
                />
            </div>

            {/* Console */}
            <div className="bg-neutral-100 dark:bg-black border border-neutral-300 dark:border-neutral-800/50 p-6">
                <div className="flex items-center gap-3 mb-6">
                    <h2 className="text-2xl text-neutral-900 dark:text-neutral-100 tracking-wider" style={{ fontFamily: "'Seven Segment', sans-serif" }}>CONSOLE</h2>
                    <div className="flex-1 h-px bg-neutral-300 dark:bg-neutral-800"></div>
                    {connecting ? (
                        <span className="text-[10px] text-yellow-600 dark:text-yellow-400" style={{ fontFamily: "'Space Mono', monospace" }}>CONNECTING...</span>
                    ) : connected ? (
                        <span className="text-[10px] text-green-600 dark:text-green-400" style={{ fontFamily: "'Space Mono', monospace" }}>● LIVE</span>
                    ) : (
                        <button onClick={connect} className="text-[10px] text-red-600 dark:text-red-400 hover:text-red-500 dark:hover:text-red-300" style={{ fontFamily: "'Space Mono', monospace" }}>○ RECONNECT</button>
                    )}
                </div>

                <div ref={logContainerRef} className="bg-neutral-50 dark:bg-neutral-900/50 border border-neutral-300 dark:border-neutral-800/50 p-4 overflow-y-auto font-mono text-sm" style={{ height: '400px', maxHeight: '400px' }} onClick={() => commandInputRef.current?.focus()}>
                    {logs.length === 0 ? (
                        <div className="text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>{connecting ? 'Connecting...' : 'Waiting for output...'}</div>
                    ) : (
                        logs.map((log) => (
                            <div key={log.id} className={`leading-relaxed break-all ${log.type === 'error' ? 'text-red-600 dark:text-red-400' : log.type === 'success' ? 'text-green-600 dark:text-green-400' : log.type === 'info' ? 'text-blue-600 dark:text-blue-400' : log.type === 'status' ? 'text-yellow-600 dark:text-yellow-400' : 'text-neutral-700 dark:text-neutral-300'}`} style={{ fontFamily: "'Space Mono', monospace" }}>
                                <span className="text-neutral-500 dark:text-neutral-600 mr-2">[{formatTimestamp(log.timestamp)}]</span>{log.text}
                            </div>
                        ))
                    )}
                </div>

                <form onSubmit={handleCommandSubmit} className="mt-4 flex gap-2">
                    <div className="flex-1 relative">
                        <span className="absolute left-3 top-1/2 -translate-y-1/2 text-neutral-500 dark:text-neutral-500 text-sm" style={{ fontFamily: "'Space Mono', monospace" }}>&gt;</span>
                        <Input ref={commandInputRef} value={command} onChange={(e) => setCommand(e.target.value)} onKeyDown={handleKeyDown} placeholder={connected ? "Enter command..." : "Connect first"} disabled={!connected} className="pl-8 text-sm bg-neutral-50 dark:bg-neutral-900/50 border-neutral-300 dark:border-neutral-800/50 text-neutral-900 dark:text-neutral-100 placeholder:text-neutral-500 dark:placeholder:text-neutral-600" style={{ fontFamily: "'Space Mono', monospace" }} />
                    </div>
                    <Button type="submit" disabled={!connected || !command.trim()} className="px-4 text-sm bg-red-600 hover:bg-red-700 disabled:bg-neutral-300 dark:disabled:bg-neutral-800 text-white border-transparent" style={{ fontFamily: "'Space Mono', monospace" }}>SEND</Button>
                </form>
            </div>

            {/* Charts - Always visible, show zero line when no data */}
            <div className="grid md:grid-cols-2 gap-4">
                <div className="bg-neutral-100 dark:bg-black border border-neutral-300 dark:border-neutral-800/50 p-6">
                    <div className="text-[10px] uppercase tracking-[0.2em] text-neutral-600 dark:text-neutral-500 mb-3" style={{ fontFamily: "'Space Mono', monospace" }}>Memory History (MB)</div>
                    <div style={{ width: '100%', height: '160px' }}>
                        <ResponsiveContainer width="100%" height="100%">
                            <AreaChart data={chartData}>
                                <defs>
                                    <linearGradient id="memGrad" x1="0" y1="0" x2="0" y2="1">
                                        <stop offset="5%" stopColor="#f59e0b" stopOpacity={0.3}/>
                                        <stop offset="95%" stopColor="#f59e0b" stopOpacity={0}/>
                                    </linearGradient>
                                </defs>
                                <XAxis dataKey="time" tick={{ fill: '#737373', fontSize: 9 }} axisLine={{ stroke: '#404040' }} tickLine={false} />
                                <YAxis tick={{ fill: '#737373', fontSize: 9 }} axisLine={{ stroke: '#404040' }} tickLine={false} width={40} domain={[0, 'auto']} />
                                <Tooltip contentStyle={{ backgroundColor: '#0a0a0a', border: '1px solid #404040', borderRadius: 0, fontFamily: "'Space Mono', monospace", fontSize: 10 }} labelStyle={{ color: '#737373' }} itemStyle={{ color: '#f59e0b' }} />
                                <Area type="monotone" dataKey="memory" stroke="#f59e0b" strokeWidth={2} fill="url(#memGrad)" />
                            </AreaChart>
                        </ResponsiveContainer>
                    </div>
                </div>

                <div className="bg-neutral-100 dark:bg-black border border-neutral-300 dark:border-neutral-800/50 p-6">
                    <div className="text-[10px] uppercase tracking-[0.2em] text-neutral-600 dark:text-neutral-500 mb-3" style={{ fontFamily: "'Space Mono', monospace" }}>CPU History (%)</div>
                    <div style={{ width: '100%', height: '160px' }}>
                        <ResponsiveContainer width="100%" height="100%">
                            <AreaChart data={chartData}>
                                <defs>
                                    <linearGradient id="cpuGrad" x1="0" y1="0" x2="0" y2="1">
                                        <stop offset="5%" stopColor="#ef4444" stopOpacity={0.3}/>
                                        <stop offset="95%" stopColor="#ef4444" stopOpacity={0}/>
                                    </linearGradient>
                                </defs>
                                <XAxis dataKey="time" tick={{ fill: '#737373', fontSize: 9 }} axisLine={{ stroke: '#404040' }} tickLine={false} />
                                <YAxis tick={{ fill: '#737373', fontSize: 9 }} axisLine={{ stroke: '#404040' }} tickLine={false} domain={[0, 100]} width={40} />
                                <Tooltip contentStyle={{ backgroundColor: '#0a0a0a', border: '1px solid #404040', borderRadius: 0, fontFamily: "'Space Mono', monospace", fontSize: 10 }} labelStyle={{ color: '#737373' }} itemStyle={{ color: '#ef4444' }} />
                                <Area type="monotone" dataKey="cpu" stroke="#ef4444" strokeWidth={2} fill="url(#cpuGrad)" />
                            </AreaChart>
                        </ResponsiveContainer>
                    </div>
                </div>
            </div>

            {/* Server Info */}
            <div className="bg-neutral-100 dark:bg-black border border-neutral-300 dark:border-neutral-800/50 p-6">
                <div className="flex items-center gap-3 mb-6">
                    <h2 className="text-2xl text-neutral-900 dark:text-neutral-100 tracking-wider" style={{ fontFamily: "'Seven Segment', sans-serif" }}>INFO</h2>
                    <div className="flex-1 h-px bg-neutral-300 dark:bg-neutral-800"></div>
                </div>
                <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-xs" style={{ fontFamily: "'Space Mono', monospace" }}>
                    <div><div className="text-[10px] uppercase tracking-[0.15em] text-neutral-600 dark:text-neutral-500 mb-1">Container</div><div className="text-neutral-900 dark:text-neutral-100 truncate">{server.containerId?.slice(0, 12) || 'N/A'}</div></div>
                    <div><div className="text-[10px] uppercase tracking-[0.15em] text-neutral-600 dark:text-neutral-500 mb-1">Node</div><div className="text-neutral-900 dark:text-neutral-100 truncate">{server.nodeId?.slice(0, 12) || server.node || 'Unknown'}</div></div>
                    <div><div className="text-[10px] uppercase tracking-[0.15em] text-neutral-600 dark:text-neutral-500 mb-1">Image</div><div className="text-neutral-900 dark:text-neutral-100 truncate">{server.dockerImage || 'N/A'}</div></div>
                    <div><div className="text-[10px] uppercase tracking-[0.15em] text-neutral-600 dark:text-neutral-500 mb-1">Software</div><div className="text-neutral-900 dark:text-neutral-100 truncate">{server.serverSoftwareId?.slice(0, 12) || 'N/A'}</div></div>
                </div>
                <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-xs mt-4" style={{ fontFamily: "'Space Mono', monospace" }}>
                    <div><div className="text-[10px] uppercase tracking-[0.15em] text-neutral-600 dark:text-neutral-500 mb-1">CPU Limit</div><div className="text-neutral-900 dark:text-neutral-100">{server.limits?.cpu || 'N/A'} vCPUs</div></div>
                    <div><div className="text-[10px] uppercase tracking-[0.15em] text-neutral-600 dark:text-neutral-500 mb-1">Memory Limit</div><div className="text-neutral-900 dark:text-neutral-100">{server.limits?.memory || 'N/A'}</div></div>
                    <div><div className="text-[10px] uppercase tracking-[0.15em] text-neutral-600 dark:text-neutral-500 mb-1">Disk Limit</div><div className="text-neutral-900 dark:text-neutral-100">{server.limits?.disk || 'N/A'}</div></div>
                    <div><div className="text-[10px] uppercase tracking-[0.15em] text-neutral-600 dark:text-neutral-500 mb-1">PIDs Limit</div><div className="text-neutral-900 dark:text-neutral-100">{server.limits?.pids || 'N/A'}</div></div>
                </div>
                {server.startup?.command && (
                    <div className="mt-4 text-xs" style={{ fontFamily: "'Space Mono', monospace" }}>
                        <div className="text-[10px] uppercase tracking-[0.15em] text-neutral-600 dark:text-neutral-500 mb-1">Startup Command</div>
                        <div className="text-neutral-900 dark:text-neutral-100 bg-neutral-50 dark:bg-neutral-900/50 border border-neutral-300 dark:border-neutral-800/50 p-3 font-mono text-[11px] break-all">{server.startup.command}</div>
                    </div>
                )}
            </div>
        </div>
    )
}
