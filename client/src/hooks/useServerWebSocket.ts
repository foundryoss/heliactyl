import { useCallback, useEffect, useRef } from 'react'
import { useServerStore } from '@/state/server'

export type WebConsoleLogType = 'stdout' | 'info' | 'error' | 'success' | 'status' | 'muted'

interface UseServerWebSocketOptions {
    onLog?: (text: string, type: WebConsoleLogType) => void
    tenantId: string | null
    serverId: string | null
    api: any
}

function stripAnsi(input: string): string {
    // Basic SGR sequences: \x1b[...m
    return input.replace(/\u001b\[[0-9;]*m/g, '')
}

function normalizeLogPrefixes(input: string): string {
    let output = input

    // Remove leading HH:MM:SS timestamp
    output = output.replace(/^\[\d{2}:\d{2}:\d{2}\]\s*/, '')

    // Remove repeated "[something]: " prefixes (e.g. [container@pkg.lat]: ...)
    while (true) {
        const match = output.match(/^\[[^\]]+\]:\s*/)
        if (!match) break
        output = output.slice(match[0].length)
    }

    // Remove old literal daemon tag if present
    output = output.replace(/^\[daemon\]\s*/i, '')

    // Remove "container@pkg.lat:" style prefixes
    output = output.replace(/^[^:\s]+@pkg\.lat:\s*/i, '')

    return output
}

function inferLogTypeFromText(text: string): WebConsoleLogType {
    const trimmed = text.trimStart()
    const lower = trimmed.toLowerCase()

    // Stack traces / call sites
    if (/^at\s+/.test(trimmed) || /^\.{3}\s*\d+\s*more$/.test(trimmed) || lower.startsWith('caused by:')) {
        return 'muted'
    }

    // Errors
    if (
        /^error:\s*/i.test(trimmed) ||
        lower.includes('module_not_found') ||
        lower.includes('unhandled') ||
        lower.includes('exception') ||
        lower.includes('traceback')
    ) {
        return 'error'
    }

    // Warnings / status-ish
    if (/^status:\s*/i.test(trimmed) || /^warn(ing)?:\s*/i.test(trimmed)) {
        return 'status'
    }

    return 'stdout'
}

function parseConsoleLine(raw: string, sourceHint?: 'daemon' | 'container'): { text: string; type: WebConsoleLogType } {
    const normalizedNewlines = raw.replace(/\r\n/g, '\n').replace(/\r/g, '\n')
    const noAnsi = stripAnsi(normalizedNewlines)
    const withoutPrefixes = normalizeLogPrefixes(noAnsi).trimEnd()

    let type = inferLogTypeFromText(withoutPrefixes)
    if (sourceHint === 'daemon' && (type === 'stdout' || type === 'info' || type === 'muted')) {
        type = 'status'
    }

    const text = sourceHint === 'daemon' ? `[Lightd] ${withoutPrefixes}` : withoutPrefixes
    return { text, type }
}

export function useServerWebSocket({ onLog, tenantId, serverId, api }: UseServerWebSocketOptions) {
    const reconnectTimeoutRef = useRef<NodeJS.Timeout | null>(null)
    const reconnectAttemptsRef = useRef(0)
    const maxReconnectAttempts = 5
    
    // Track connection state with refs to avoid race conditions
    const connectedServerIdRef = useRef<string | null>(null)
    const connectionStateRef = useRef<'idle' | 'connecting' | 'connected'>('idle')
    const wsRef = useRef<WebSocket | null>(null)
    const isMountedRef = useRef(true)
    
    // Store current values in refs to avoid dependency issues
    const tenantIdRef = useRef(tenantId)
    const serverIdRef = useRef(serverId)
    const apiRef = useRef(api)
    const onLogRef = useRef(onLog)
    
    // Update refs when props change
    tenantIdRef.current = tenantId
    serverIdRef.current = serverId
    apiRef.current = api
    onLogRef.current = onLog
    
    // Get state and actions from store (for UI updates only)
    const connected = useServerStore(state => state.connected)
    const connecting = useServerStore(state => state.connecting)
    const status = useServerStore(state => state.status)
    const setConnected = useServerStore(state => state.setConnected)
    const setConnecting = useServerStore(state => state.setConnecting)
    const setStatus = useServerStore(state => state.setStatus)
    const setStats = useServerStore(state => state.setStats)
    const addStatsToHistory = useServerStore(state => state.addStatsToHistory)
    const setWebSocket = useServerStore(state => state.setWebSocket)
    const reset = useServerStore(state => state.reset)

    const handleMessage = useCallback((event: MessageEvent) => {
        try {
            const data = JSON.parse(event.data)
            
            switch (data.event) {
                case 'init':
                    if (data.args?.length >= 3) {
                        connectionStateRef.current = 'connected'
                        setConnected(true)
                        setConnecting(false)
                        setStatus(data.args[2] as any)
                        onLogRef.current?.('Connected to container', 'success')
                        reconnectAttemptsRef.current = 0
                        
                        // Reset stats if stopped
                        if (['stopped', 'offline', 'exited'].includes(data.args[2])) {
                            setStats(null)
                        }
                    }
                    break
                    
                case 'console_output':
                    if (data.args?.[0]) {
                        const parsed = parseConsoleLine(String(data.args[0]), 'container')
                        if (parsed.text) {
                            onLogRef.current?.(parsed.text, parsed.type)
                        }
                    }
                    break
                    
                case 'status':
                    if (data.args?.[0]) {
                        const newStatus = data.args[0]
                        setStatus(newStatus)
                        onLogRef.current?.(`[Lightd] Status: ${String(newStatus).toUpperCase()}`, 'status')
                        
                        // Reset stats when container stops
                        if (['stopped', 'offline', 'exited', 'killed'].includes(newStatus)) {
                            setStats(null)
                        }
                    }
                    break
                    
                case 'stats':
                    try {
                        const statsData = typeof data.args[0] === 'string' 
                            ? JSON.parse(data.args[0]) 
                            : data.args[0]
                        
                        setStats(statsData)
                        
                        // Add to history
                        const memoryMb = statsData.memory_bytes / (1024 * 1024)
                        const cpuPercent = statsData.cpu_absolute
                        addStatsToHistory(memoryMb, cpuPercent)
                    } catch (e) {
                        console.error('Failed to parse stats:', e)
                    }
                    break
                    
                case 'daemon_message':
                    if (data.args?.[0]) {
                        const parsed = parseConsoleLine(String(data.args[0]), 'daemon')
                        if (parsed.text) {
                            onLogRef.current?.(parsed.text, parsed.type)
                        }
                    }
                    break
                    
                case 'error':
                    if (data.args?.[0]) {
                        onLogRef.current?.(`Error: ${data.args[0]}`, 'error')
                    }
                    break
                    
                default:
                    if (data.args?.length > 0) {
                        onLogRef.current?.(`[${data.event}] ${data.args.join(' ')}`, 'info')
                    }
            }
        } catch (e) {
            // If not JSON, treat as raw log output
            const parsed = parseConsoleLine(String(event.data), 'container')
            if (parsed.text) {
                onLogRef.current?.(parsed.text, parsed.type)
            }
        }
    }, [setConnected, setConnecting, setStatus, setStats, addStatsToHistory])

    const handleClose = useCallback((event: CloseEvent) => {
        // Only process if this is our current WebSocket
        connectionStateRef.current = 'idle'
        wsRef.current = null
        setConnected(false)
        setConnecting(false)
        
        // Don't reconnect if unmounted
        if (!isMountedRef.current) return
        
        // Only attempt reconnection if we were connected to a server
        const currentServerId = connectedServerIdRef.current
        if (!currentServerId) {
            onLogRef.current?.(`Disconnected (code: ${event.code})`, 'info')
            return
        }
        
        onLogRef.current?.(`Disconnected (code: ${event.code})`, 'info')
        
        // Don't reconnect on normal close
        if (event.code === 1000) {
            connectedServerIdRef.current = null
            return
        }
        
        // Attempt reconnection with exponential backoff
        if (reconnectAttemptsRef.current < maxReconnectAttempts) {
            const delay = Math.min(1000 * Math.pow(2, reconnectAttemptsRef.current), 10000)
            reconnectAttemptsRef.current++
            
            onLogRef.current?.(`Reconnecting in ${delay / 1000}s... (attempt ${reconnectAttemptsRef.current}/${maxReconnectAttempts})`, 'info')
            
            reconnectTimeoutRef.current = setTimeout(() => {
                // Double check we're still meant to be connected to this server
                if (isMountedRef.current && 
                    connectedServerIdRef.current === currentServerId && 
                    connectionStateRef.current === 'idle') {
                    connectToServer()
                }
            }, delay)
        } else {
            onLogRef.current?.('Max reconnection attempts reached', 'error')
            connectedServerIdRef.current = null
        }
    }, [setConnected, setConnecting])

    const handleError = useCallback(() => {
        connectionStateRef.current = 'idle'
        setConnected(false)
        setConnecting(false)
        onLogRef.current?.('Connection error', 'error')
    }, [setConnected, setConnecting])

    // Internal connect function that uses refs (stable, no deps on props)
    const connectToServer = useCallback(async () => {
        const currentTenantId = tenantIdRef.current
        const currentServerId = serverIdRef.current
        const currentApi = apiRef.current
        
        if (!currentTenantId || !currentServerId) return
        
        // Prevent duplicate connections - use ref state, not store state
        if (connectionStateRef.current !== 'idle') {
            console.log('[WS] Already connecting or connected, skipping')
            return
        }
        
        // If we have an existing WebSocket to a different server, close it
        if (wsRef.current && connectedServerIdRef.current !== currentServerId) {
            wsRef.current.close(1000, 'Switching servers')
            wsRef.current = null
            connectedServerIdRef.current = null
        }
        
        // Clear any pending reconnection
        if (reconnectTimeoutRef.current) {
            clearTimeout(reconnectTimeoutRef.current)
            reconnectTimeoutRef.current = null
        }
        
        connectionStateRef.current = 'connecting'
        setConnecting(true)
        onLogRef.current?.('Generating token...', 'info')
        
        try {
            const credentials = await currentApi.servers.websocket(currentTenantId, currentServerId)
            
            // Check if unmounted or server changed while fetching token
            if (!isMountedRef.current) {
                connectionStateRef.current = 'idle'
                setConnecting(false)
                return
            }
            
            if (serverIdRef.current !== currentServerId) {
                connectionStateRef.current = 'idle'
                setConnecting(false)
                return
            }
            
            onLogRef.current?.('Connecting to console...', 'info')
            
            const newWs = new WebSocket(credentials.socket)
            wsRef.current = newWs
            connectedServerIdRef.current = currentServerId
            
            newWs.onopen = () => {
                if (isMountedRef.current && serverIdRef.current === currentServerId) {
                    onLogRef.current?.('WebSocket connected, waiting for init...', 'success')
                } else {
                    // Server changed or unmounted, close this connection
                    newWs.close(1000, 'Server changed')
                }
            }
            
            newWs.onmessage = handleMessage
            newWs.onclose = handleClose
            newWs.onerror = handleError
            
            setWebSocket(newWs)
        } catch (e: any) {
            connectionStateRef.current = 'idle'
            setConnecting(false)
            onLogRef.current?.(`Failed to connect: ${e?.message || 'Unknown error'}`, 'error')
        }
    }, [handleMessage, handleClose, handleError, setConnecting, setWebSocket])

    // Public connect function
    const connect = useCallback(() => {
        connectToServer()
    }, [connectToServer])

    const disconnect = useCallback(() => {
        // Clear reconnection attempts
        if (reconnectTimeoutRef.current) {
            clearTimeout(reconnectTimeoutRef.current)
            reconnectTimeoutRef.current = null
        }
        
        // Clear server tracking
        connectedServerIdRef.current = null
        connectionStateRef.current = 'idle'
        reconnectAttemptsRef.current = 0
        
        if (wsRef.current) {
            wsRef.current.close(1000, 'User disconnect')
            wsRef.current = null
        }
        
        setConnected(false)
        setConnecting(false)
        setWebSocket(null)
    }, [setWebSocket, setConnected, setConnecting])

    const sendCommand = useCallback((command: string) => {
        if (!wsRef.current || connectionStateRef.current !== 'connected' || !command.trim()) return false
        
        try {
            wsRef.current.send(JSON.stringify({ event: 'send_command', args: [command.trim()] }))
            return true
        } catch (e) {
            onLogRef.current?.('Failed to send command', 'error')
            return false
        }
    }, [])

    // Handle server change - disconnect from old server and connect to new one
    useEffect(() => {
        if (!serverId || !tenantId) {
            // No server selected, disconnect if connected
            if (connectedServerIdRef.current || wsRef.current) {
                // Clear reconnection
                if (reconnectTimeoutRef.current) {
                    clearTimeout(reconnectTimeoutRef.current)
                    reconnectTimeoutRef.current = null
                }
                
                if (wsRef.current) {
                    wsRef.current.close(1000, 'No server')
                    wsRef.current = null
                }
                
                connectedServerIdRef.current = null
                connectionStateRef.current = 'idle'
                reconnectAttemptsRef.current = 0
                setConnected(false)
                setConnecting(false)
                setWebSocket(null)
                reset()
            }
            return
        }
        
        // Server changed - need to reconnect
        if (connectedServerIdRef.current && connectedServerIdRef.current !== serverId) {
            onLogRef.current?.('Switching servers...', 'info')
            
            // Clear old state
            if (reconnectTimeoutRef.current) {
                clearTimeout(reconnectTimeoutRef.current)
                reconnectTimeoutRef.current = null
            }
            
            // Close old connection
            if (wsRef.current) {
                wsRef.current.close(1000, 'Server changed')
                wsRef.current = null
            }
            
            // Reset state for new server
            connectedServerIdRef.current = null
            connectionStateRef.current = 'idle'
            reconnectAttemptsRef.current = 0
            setWebSocket(null)
            reset()
            
            // Connect to new server after a brief delay
            setTimeout(() => {
                if (isMountedRef.current) {
                    connectToServer()
                }
            }, 100)
        }
    }, [serverId, tenantId, connectToServer, setConnected, setConnecting, setWebSocket, reset])

    // Set mounted flag and cleanup on unmount
    useEffect(() => {
        isMountedRef.current = true
        
        return () => {
            isMountedRef.current = false
            
            // Clear reconnection
            if (reconnectTimeoutRef.current) {
                clearTimeout(reconnectTimeoutRef.current)
                reconnectTimeoutRef.current = null
            }
            
            // Close websocket
            if (wsRef.current) {
                wsRef.current.close(1000, 'Unmounting')
                wsRef.current = null
            }
            
            connectedServerIdRef.current = null
            connectionStateRef.current = 'idle'
        }
    }, [])

    return {
        connect,
        disconnect,
        sendCommand,
        connected,
        connecting,
        status,
        connectedServerId: connectedServerIdRef.current
    }
}
