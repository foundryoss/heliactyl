import React, { useCallback, useEffect, useRef, useState } from 'react'
import { useAuth } from '@/providers/AuthProvider'
import { useApi } from '@/api/client'
import { useTenants } from '@/providers/TenantProvider'
import { useParams, useNavigate } from 'react-router-dom'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card'
import { Button } from '@/components/ui/Button'
import { Input } from '@/components/ui/Input'
import Spinner from '@/components/ui/Spinner'
import { useAlert } from '@/components/ui/Alert'
import { 
    PlayIcon, 
    StopIcon, 
    ArrowPathIcon,
    XMarkIcon,
    ExclamationTriangleIcon,
    ArrowUpRightIcon,
    CommandLineIcon
} from '@heroicons/react/24/outline'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import '../../xterm.css'

interface ServerStats {
    memory_bytes: number
    memory_limit_bytes: number
    cpu_absolute: number
    network: {
        rx_bytes: number
        tx_bytes: number
    }
    state: string
    disk_bytes: number
}

interface ServerResources {
    memory: number
    swap: number
    disk: number
    io: number
    cpu: number
}

// ANSI escape code parser for fallback terminal
function parseAnsiToHtml(text: string): string {
    // ANSI color map
    const colors: Record<number, string> = {
        30: '#000000', // black
        31: '#ff5555', // red
        32: '#50fa7b', // green
        33: '#f1fa8c', // yellow
        34: '#bd93f9', // blue
        35: '#ff79c6', // magenta
        36: '#8be9fd', // cyan
        37: '#f8f8f2', // white
        90: '#6272a4', // bright black (gray)
        91: '#ff6e67', // bright red
        92: '#5af78e', // bright green
        93: '#f4f99d', // bright yellow
        94: '#caa9fa', // bright blue
        95: '#ff92d0', // bright magenta
        96: '#9aedfe', // bright cyan
        97: '#ffffff'  // bright white
    }

    let html = ''
    let currentColor = '#ffffff'
    let isBold = false
    let isUnderline = false
    
    // Split by escape sequences
    const parts = text.split(/\x1b\[([0-9;]*[a-zA-Z])/)
    
    for (let i = 0; i < parts.length; i++) {
        if (i % 2 === 0) {
            // Regular text
            if (parts[i]) {
                const escapedText = parts[i]
                    .replace(/&/g, '&amp;')
                    .replace(/</g, '&lt;')
                    .replace(/>/g, '&gt;')
                    .replace(/"/g, '&quot;')
                    .replace(/'/g, '&#39;')
                
                const style = `color: ${currentColor}; ${isBold ? 'font-weight: bold;' : ''} ${isUnderline ? 'text-decoration: underline;' : ''}`
                html += `<span style="${style}">${escapedText}</span>`
            }
        } else {
            // ANSI escape sequence
            const sequence = parts[i]
            if (sequence) {
                const codes = sequence.slice(0, -1).split(';').map(Number).filter(code => !isNaN(code))
                
                for (const code of codes) {
                    if (code === 0) {
                        // Reset
                        currentColor = '#ffffff'
                        isBold = false
                        isUnderline = false
                    } else if (code === 1) {
                        // Bold
                        isBold = true
                    } else if (code === 4) {
                        // Underline
                        isUnderline = true
                    } else if (code === 22) {
                        // Bold off
                        isBold = false
                    } else if (code === 24) {
                        // Underline off
                        isUnderline = false
                    } else if (colors[code]) {
                        // Color code
                        currentColor = colors[code]
                    }
                }
            }
        }
    }
    
    return html || text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
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
    const [serverStatus, setServerStatus] = useState<string>('unknown')
    const [stats, setStats] = useState<ServerStats | null>(null)
    const [resources, setResources] = useState<ServerResources | null>(null)
    const [command, setCommand] = useState('')
    const [commandHistory, setCommandHistory] = useState<string[]>([])
    const [historyIndex, setHistoryIndex] = useState(-1)
    const [hasLogs, setHasLogs] = useState(false)
    const [connectionFailed, setConnectionFailed] = useState(false)

    const wsRef = useRef<WebSocket | null>(null)
    const terminalRef = useRef<HTMLDivElement>(null)
    const terminalInstanceRef = useRef<Terminal | null>(null)
    const fitAddonRef = useRef<FitAddon | null>(null)
    const commandInputRef = useRef<HTMLInputElement>(null)
    const reconnectTimeoutRef = useRef<NodeJS.Timeout | undefined>(undefined)
    const tokenExpiryTimeoutRef = useRef<NodeJS.Timeout | undefined>(undefined)

    const initializeTerminal = useCallback(() => {
        if (!terminalRef.current) return

        try {
            const terminal = new Terminal({
                fontFamily: '"Space Mono", "SF Mono", Monaco, Inconsolata, "Fira Code", "Fira Mono", "Roboto Mono", monospace',
                fontSize: 14,
                fontWeight: '400',
                lineHeight: 1.2,
                letterSpacing: 0,
                theme: {
                    background: '#000000',
                    foreground: '#ffffff',
                    cursor: '#ffffff',
                    black: '#000000',
                    red: '#ff5555',
                    green: '#50fa7b',
                    yellow: '#f1fa8c',
                    blue: '#bd93f9',
                    magenta: '#ff79c6',
                    cyan: '#8be9fd',
                    white: '#bfbfbf',
                    brightBlack: '#4d4d4d',
                    brightRed: '#ff6e67',
                    brightGreen: '#5af78e',
                    brightYellow: '#f4f99d',
                    brightBlue: '#caa9fa',
                    brightMagenta: '#ff92d0',
                    brightCyan: '#9aedfe',
                    brightWhite: '#e6e6e6'
                },
                cursorBlink: false,
                disableStdin: true,
                convertEol: true,
                scrollback: 1000,
                allowProposedApi: true
            })

            const fitAddon = new FitAddon()
            const webLinksAddon = new WebLinksAddon()
            
            terminal.loadAddon(fitAddon)
            terminal.loadAddon(webLinksAddon)
            terminal.open(terminalRef.current)
            
            setTimeout(() => {
                fitAddon.fit()
            }, 100)

            terminalInstanceRef.current = terminal
            fitAddonRef.current = fitAddon

            const resizeObserver = new ResizeObserver(() => {
                try {
                    fitAddon.fit()
                } catch (e) {
                    // Ignore resize errors
                }
            })
            resizeObserver.observe(terminalRef.current)

            return () => {
                resizeObserver.disconnect()
                terminal.dispose()
            }
        } catch (error) {
            // Silent fallback
            setConnectionFailed(true)
        }
    }, [])

    const addConsoleOutput = useCallback((text: string) => {
        setHasLogs(true)
        
        if (terminalInstanceRef.current) {
            terminalInstanceRef.current.writeln(text)
        } else if (terminalRef.current) {
            const logLine = document.createElement('div')
            logLine.style.fontFamily = '"Space Mono", monospace'
            logLine.style.fontSize = '14px'
            logLine.style.marginBottom = '2px'
            logLine.style.whiteSpace = 'pre-wrap'
            logLine.innerHTML = parseAnsiToHtml(text)
            
            terminalRef.current.appendChild(logLine)
            terminalRef.current.scrollTop = terminalRef.current.scrollHeight
        }
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
            setResources({
                memory: foundServer.memoryMb,
                swap: -1,
                disk: foundServer.diskMb,
                io: 500,
                cpu: foundServer.cpuPercent
            })
        } catch (e: any) {
            setError(e?.message || 'Failed to load server')
        } finally {
            setLoading(false)
        }
    }, [api, selectedTenantId, serverId])

    const getWebSocketCredentials = useCallback(async () => {
        if (!server) return null
        
        try {
            const response = await api.servers.websocket(selectedTenantId!, server.id)
            return response
        } catch (e: any) {
            setConnectionFailed(true)
            return null
        }
    }, [api, selectedTenantId, server])

    const connectWebSocket = useCallback(async () => {
        if (!server) return

        const credentials = await getWebSocketCredentials()
        if (!credentials) return

        try {
            setConnectionFailed(false)

            const ws = new WebSocket(credentials.socket)
            wsRef.current = ws

            ws.onopen = () => {
                ws.send(JSON.stringify({
                    event: 'auth',
                    args: [credentials.token]
                }))
            }

            ws.onmessage = (event) => {
                try {
                    const data = JSON.parse(event.data)
                    
                    switch (data.event) {
                        case 'auth success':
                            setConnected(true)
                            setConnectionFailed(false)
                            ws.send(JSON.stringify({ event: 'send stats', args: [null] }))
                            ws.send(JSON.stringify({ event: 'send logs', args: [null] }))
                            break

                        case 'status':
                            setServerStatus(data.args[0])
                            break

                        case 'console output':
                            addConsoleOutput(data.args[0])
                            break

                        case 'stats':
                            try {
                                const statsData = JSON.parse(data.args[0])
                                setStats(statsData)
                            } catch (e) {
                                // Ignore stats parsing errors
                            }
                            break

                        case 'token expiring':
                            tokenExpiryTimeoutRef.current = setTimeout(async () => {
                                const newCredentials = await getWebSocketCredentials()
                                if (newCredentials && ws.readyState === WebSocket.OPEN) {
                                    ws.send(JSON.stringify({
                                        event: 'auth',
                                        args: [newCredentials.token]
                                    }))
                                }
                            }, 30000)
                            break

                        case 'token expired':
                            connectWebSocket()
                            break
                    }
                } catch (e) {
                    // Ignore parsing errors
                }
            }

            ws.onclose = () => {
                setConnected(false)
                
                if (ws === wsRef.current && !connectionFailed) {
                    reconnectTimeoutRef.current = setTimeout(() => {
                        connectWebSocket()
                    }, 3000)
                }
            }

            ws.onerror = () => {
                setConnected(false)
                setConnectionFailed(true)
            }

        } catch (e: any) {
            setConnectionFailed(true)
        }
    }, [server, getWebSocketCredentials, addConsoleOutput, connectionFailed])

    const disconnectWebSocket = useCallback(() => {
        if (wsRef.current) {
            wsRef.current.close()
            wsRef.current = null
        }
        if (reconnectTimeoutRef.current) {
            clearTimeout(reconnectTimeoutRef.current)
        }
        if (tokenExpiryTimeoutRef.current) {
            clearTimeout(tokenExpiryTimeoutRef.current)
        }
        setConnected(false)
    }, [])

    const sendCommand = useCallback((cmd: string) => {
        if (!wsRef.current || !connected || !cmd.trim()) return

        const trimmedCmd = cmd.trim()
        wsRef.current.send(JSON.stringify({
            event: 'send command',
            args: [trimmedCmd]
        }))

        setCommandHistory(prev => {
            const newHistory = [trimmedCmd, ...prev.filter(c => c !== trimmedCmd)]
            return newHistory.slice(0, 50)
        })
        setHistoryIndex(-1)
        setCommand('')
    }, [connected])

    const sendPowerAction = useCallback((action: string) => {
        if (!wsRef.current || !connected) return

        const actionNames: Record<string, string> = {
            start: 'Starting',
            stop: 'Stopping', 
            restart: 'Restarting',
            kill: 'Force stopping'
        }

        wsRef.current.send(JSON.stringify({
            event: 'set state',
            args: [action]
        }))

        notify({ description: `${actionNames[action] || action} server...`, type: 'success' })
    }, [connected, notify])

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
                const newIndex = historyIndex - 1
                setHistoryIndex(newIndex)
                setCommand(commandHistory[newIndex])
            } else if (historyIndex === 0) {
                setHistoryIndex(-1)
                setCommand('')
            }
        }
    }, [historyIndex, commandHistory])

    const formatBytes = useCallback((bytes: number) => {
        if (bytes === 0) return '0 B'
        const k = 1024
        const sizes = ['B', 'KB', 'MB', 'GB']
        const i = Math.floor(Math.log(bytes) / Math.log(k))
        return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i]
    }, [])

    const getStatusColor = useCallback((status: string) => {
        switch (status.toLowerCase()) {
            case 'running': return 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200'
            case 'starting': return 'bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200'
            case 'stopping': return 'bg-orange-100 text-orange-800 dark:bg-orange-900 dark:text-orange-200'
            case 'offline': return 'bg-gray-100 text-gray-800 dark:bg-gray-900 dark:text-gray-200'
            default: return 'bg-gray-100 text-gray-800 dark:bg-gray-900 dark:text-gray-200'
        }
    }, [])

    const getPowerButton = useCallback(() => {
        const isOffline = serverStatus === 'offline'
        const isStarting = serverStatus === 'starting'
        const isStopping = serverStatus === 'stopping'

        if (isOffline || isStopping) {
            return (
                <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => sendPowerAction('start')}
                    disabled={!connected || isStarting}
                    className="text-green-600 hover:text-green-900 dark:text-green-400"
                >
                    <PlayIcon className="w-4 h-4" />
                    Start
                </Button>
            )
        }

        return (
            <>
                <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => sendPowerAction('restart')}
                    disabled={!connected || isOffline}
                    className="text-orange-600 flex items-center gap-1 hover:text-orange-900 dark:text-orange-400"
                >
                    <ArrowPathIcon className="w-4 h-4" />
                    Restart
                </Button>
                {isStopping ? (
                    <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => sendPowerAction('kill')}
                        disabled={!connected}
                        className="text-red-600 flex items-center gap-1 hover:text-red-900 dark:text-red-400"
                    >
                        <XMarkIcon className="w-4 h-4" />
                        Kill
                    </Button>
                ) : (
                    <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => sendPowerAction('stop')}
                        disabled={!connected || isOffline}
                        className="text-red-600 flex items-center gap-1 hover:text-red-900 dark:text-red-400"
                    >
                        <StopIcon className="w-4 h-4" />
                        Stop
                    </Button>
                )}
            </>
        )
    }, [connected, serverStatus, sendPowerAction])

    useEffect(() => {
        loadServer()
    }, [loadServer])

    useEffect(() => {
        const cleanup = initializeTerminal()
        return cleanup
    }, [initializeTerminal])

    useEffect(() => {
        if (server) {
            connectWebSocket()
        }

        return () => {
            disconnectWebSocket()
        }
    }, [server, connectWebSocket, disconnectWebSocket])

    if (!selectedTenantId) {
        return (
			<Card className="relative">
                <CardHeader>
                    <CardTitle>Select a tenant</CardTitle>
                </CardHeader>
            </Card>
        )
    }

    if (loading) {
        return (
            <div className="flex items-center justify-center py-12">
                <Spinner size="lg" />
            </div>
        )
    }

    if (error) {
        return (
			<Card className="relative overflow-hidden">
                <CardHeader>
                    <CardTitle className="text-red-600">{error}</CardTitle>
                </CardHeader>
            </Card>
        )
    }

    if (!server) {
        return (
			<Card className="relative overflow-hidden">
                <CardHeader>
                    <CardTitle>Server not found</CardTitle>
                </CardHeader>
            </Card>
        )
    }

    return (
        <div className="relative">
            {/* Connection Warning Overlay */}
            {connectionFailed && (
                <div className="fixed inset-0 backdrop-blur z-50 flex items-center justify-center">
                    <Card className="max-w-md mx-4">
                        <CardContent className="p-6 text-center">
                            <ExclamationTriangleIcon className="w-12 h-12 text-amber-500 mx-auto mb-4" />
                            <h3 className="text-lg font-semibold text-neutral-900 dark:text-neutral-100 mb-2">
                                Connection failed
                            </h3>
                            <p className="text-neutral-600 dark:text-neutral-400 mb-6">
                                Unable to connect to the server console. Please check your connection and try again.
                            </p>
                            <div className="flex gap-3 justify-center">
                                <Button
                                    variant="ghost"
                                    onClick={() => navigate('/servers')}
                                >
                                    Back to Servers
                                </Button>
                                <Button
                                    onClick={() => {
                                        setConnectionFailed(false)
                                        connectWebSocket()
                                    }}
                                >
                                    Retry Connection
                                </Button>
                            </div>
                        </CardContent>
                    </Card>
                </div>
            )}

            <div className={`space-y-6 ${connectionFailed ? 'blur-sm' : ''}`}>
                {/* Header */}
                <div className="flex items-start justify-between">
                    <div className="space-y-2">
                        <div className="flex items-center gap-3">
                            <h1 className="text-2xl font-semibold text-neutral-900 dark:text-neutral-100">
                                {server.name}
                            </h1>
                            <div className={`inline-flex items-center px-3 py-1 rounded-full text-xs font-semibold uppercase tracking-wide ${getStatusColor(serverStatus)}`}>
                                <div className="w-1.5 h-1.5 rounded-full bg-current mr-2 animate-pulse" />
                                {serverStatus}
                            </div>
                        </div>
                        <div className="flex items-center gap-6 text-sm">
                            <div className="flex items-center gap-2 text-neutral-600 dark:text-neutral-400">
                                <span className="font-medium">IP:</span>
                                <span className="font-mono bg-neutral-100 dark:bg-neutral-800 px-2 py-0.5 rounded text-xs">
                                    {server.pteroServerId}
                                </span>
                            </div>
                            <div className="flex items-center gap-2 text-neutral-600 dark:text-neutral-400">
                                <span className="font-medium">Node:</span>
                                <span className="font-mono bg-neutral-100 dark:bg-neutral-800 px-2 py-0.5 rounded text-xs">
                                    {server.node || 'Unknown'}
                                </span>
                            </div>
                        </div>
                    </div>
                    
                    <div className="flex items-center gap-2 flex-wrap">
                        {getPowerButton()}
                    </div>
                </div>

            {/* Stats */}
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
                <Card className="relative overflow-hidden">
                    <CardContent className="p-4">
                        <div className="flex mt-4 items-center gap-2 text-sm font-medium text-neutral-500 dark:text-neutral-400">
                            Memory
                        </div>
                        <div className="text-lg font-semibold text-neutral-900 dark:text-neutral-100">
                            {stats ? formatBytes(stats.memory_bytes) : 'N/A'}
                        </div>
                        <div className="text-xs text-neutral-500 dark:text-neutral-400">
                            Limit: {resources ? formatBytes(resources.memory * 1024 * 1024) : 'N/A'}
                        </div>
                    </CardContent>
					{stats && resources && (
						<div className="absolute bottom-0 left-0 right-0 h-0.5 bg-neutral-100 dark:bg-neutral-800 rounded-full">
							<div
								className="relative h-full bg-black dark:bg-white rounded-full transition-all duration-700 ease-in-out"
								style={{ width: `${Math.min(100, (stats.memory_bytes / (resources.memory * 1024 * 1024)) * 100)}%` }}
							>
								<div className="absolute -top-3 left-0 right-0 h-3 bg-gradient-to-t from-black/30 dark:from-white/40 to-transparent pointer-events-none blur-sm" />
							</div>
						</div>
					)}
                </Card>

                <Card className="relative overflow-hidden">
                    <CardContent className="p-4">
                        <div className="flex mt-4 items-center gap-2 text-sm font-medium text-neutral-500 dark:text-neutral-400">
                            CPU
                        </div>
                        <div className="text-lg font-semibold text-neutral-900 dark:text-neutral-100">
                            {stats ? `${stats.cpu_absolute.toFixed(1)}%` : 'N/A'}
                        </div>
                        <div className="text-xs text-neutral-500 dark:text-neutral-400">
                            Limit: {resources ? `${resources.cpu}%` : 'N/A'}
                        </div>
                    </CardContent>
					{stats && resources && (
						<div className="absolute bottom-0 left-0 right-0 h-0.5 bg-neutral-100 dark:bg-neutral-800 rounded-full">
							<div
								className="relative h-full bg-black dark:bg-white rounded-full transition-all duration-700 ease-in-out"
								style={{ width: `${Math.min(100, (stats.cpu_absolute / resources.cpu) * 100)}%` }}
							>
								<div className="absolute -top-3 left-0 right-0 h-3 bg-gradient-to-t from-black/30 dark:from-white/40 to-transparent pointer-events-none blur-sm" />
							</div>
						</div>
					)}
                </Card>

                <Card className="relative overflow-hidden">
                    <CardContent className="p-4">
                        <div className="flex mt-4 items-center gap-2 text-sm font-medium text-neutral-500 dark:text-neutral-400">
                            Disk
                        </div>
                        <div className="text-lg font-semibold text-neutral-900 dark:text-neutral-100">
                            {stats ? formatBytes(stats.disk_bytes) : 'N/A'}
                        </div>
                        <div className="text-xs text-neutral-500 dark:text-neutral-400">
                            Limit: {resources ? formatBytes(resources.disk * 1024 * 1024) : 'N/A'}
                        </div>
                    </CardContent>
					{stats && resources && (
						<div className="absolute bottom-0 left-0 right-0 h-0.5 bg-neutral-100 dark:bg-neutral-800 rounded-full">
							<div
								className="relative h-full bg-black dark:bg-white rounded-full transition-all duration-700 ease-in-out"
								style={{ width: `${Math.min(100, (stats.disk_bytes / (resources.disk * 1024 * 1024)) * 100)}%` }}
							>
								<div className="absolute -top-3 left-0 right-0 h-3 bg-gradient-to-t from-black/30 dark:from-white/40 to-transparent pointer-events-none blur-sm" />
							</div>
						</div>
					)}
                </Card>

                <Card className="relative overflow-hidden">
                    <CardContent className="p-4">
                        <div className="flex mt-4 items-center gap-2 text-sm font-medium text-neutral-500 dark:text-neutral-400">
                            Network
                        </div>
                        <div className="text-sm text-neutral-900 dark:text-neutral-100">
                            ↓ {stats ? formatBytes(stats.network.rx_bytes) : 'N/A'}
                        </div>
                        <div className="text-sm text-neutral-900 dark:text-neutral-100">
                            ↑ {stats ? formatBytes(stats.network.tx_bytes) : 'N/A'}
                        </div>
                    </CardContent>
					{stats && (stats.network.rx_bytes + stats.network.tx_bytes > 0) && (
						<div className="absolute bottom-0 left-0 right-0 h-0.5">
							<div className="absolute inset-0 rounded-full bg-neutral-100 dark:bg-neutral-800" />
							<div
								className="absolute left-0 top-0 bottom-0 bg-yellow-400 rounded-l-full transition-all duration-700 ease-in-out"
								style={{ width: `${(stats.network.rx_bytes / (stats.network.rx_bytes + stats.network.tx_bytes)) * 100}%` }}
							>
								<div className="absolute -top-2 left-0 right-0 h-2 bg-gradient-to-t from-yellow-300/40 to-transparent pointer-events-none blur-sm" />
							</div>
							<div
								className="absolute right-0 top-0 bottom-0 bg-sky-400 rounded-r-full transition-all duration-700 ease-in-out"
								style={{ width: `${(stats.network.tx_bytes / (stats.network.rx_bytes + stats.network.tx_bytes)) * 100}%` }}
							>
								<div className="absolute -top-2 left-0 right-0 h-2 bg-gradient-to-t from-sky-300/40 to-transparent pointer-events-none blur-sm" />
							</div>
						</div>
					)}
                </Card>
            </div>

            {/* Console */}
            <Card className="h-[600px] flex flex-col p-0">
                <CardContent className="flex-1 bg-black rounded-lg flex flex-col p-0">
                    {/* Terminal */}
                    <div className="flex-1 bg-black relative">
                        <div 
                            ref={terminalRef} 
                            className="w-full h-full bg-black rounded overflow-y-auto"
                            style={{ 
                                fontFamily: '"Space Mono", "SF Mono", Monaco, "Cascadia Code", "Roboto Mono", Consolas, "Courier New", monospace',
                                minHeight: '400px',
                                maxHeight: '400px',
                                padding: '10px',
                                fontSize: '14px',
                                color: '#ffffff'
                            }}
                        />
                        
                        {/* Empty State */}
                        {!hasLogs && connected && (
                            <div className="absolute inset-4 flex items-center justify-center">
                                <div className="text-center text-neutral-400">
                                    <div className="flex items-center justify-center">
                                        <div className="flex rounded-lg p-4 bg-white/10">
                                            <CommandLineIcon className="w-5 h-5" />
                                        </div>
                                    </div>
                                    <p className="text-lg font-medium text-white mt-8">Hmm... nothing yet.</p>
                                    <p className="text-sm">Console output will appear here</p>
                                </div>
                            </div>
                        )}
                    </div>
                    
                    {/* Command Input */}
                    <div>
                        <form onSubmit={handleCommandSubmit} className="flex items-center">
                            <input
                                ref={commandInputRef}
                                value={command}
                                onChange={(e) => setCommand(e.target.value)}
                                onKeyDown={handleKeyDown}
                                placeholder={connected ? "$" : "Not connected to server"}
                                disabled={!connected}
                                className="flex-1 bg-white/10 px-3 py-3 rounded-l-xl placeholder-neutral-400 outline-none text-white"
                                style={{ fontSize: '14px', fontWeight: '500' }}
                            />
                            <button
                                type="submit"
                                disabled={!connected || !command.trim()}
                                className="bg-white/10 px-4 border-l border-white/5 py-4 hover:bg-white/15 transition-all duration-300 cursor-pointer hover:text-white rounded-r-xl placeholder-neutral-400 outline-none text-white"
                                style={{ fontSize: '14px', fontWeight: '500' }}
                            >
                                <ArrowUpRightIcon className="w-3 h-3 text-neutral-300" />
                            </button>
                        </form>
                    </div>
                </CardContent>
            </Card>
            </div>
        </div>
    )
}