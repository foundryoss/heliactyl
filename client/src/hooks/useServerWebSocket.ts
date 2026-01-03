import { useCallback, useEffect, useRef } from 'react'
import { useServerStore } from '@/state/server'

interface UseServerWebSocketOptions {
    onLog?: (text: string, type: 'stdout' | 'info' | 'error' | 'success' | 'status') => void
    tenantId: string | null
    serverId: string | null
    api: any
}

export function useServerWebSocket({ onLog, tenantId, serverId, api }: UseServerWebSocketOptions) {
    const reconnectTimeoutRef = useRef<NodeJS.Timeout>()
    const reconnectAttemptsRef = useRef(0)
    const maxReconnectAttempts = 5
    
    // Get state and actions from store
    const connected = useServerStore(state => state.connected)
    const connecting = useServerStore(state => state.connecting)
    const status = useServerStore(state => state.status)
    const ws = useServerStore(state => state.ws)
    const setConnected = useServerStore(state => state.setConnected)
    const setConnecting = useServerStore(state => state.setConnecting)
    const setStatus = useServerStore(state => state.setStatus)
    const setStats = useServerStore(state => state.setStats)
    const addStatsToHistory = useServerStore(state => state.addStatsToHistory)
    const setWebSocket = useServerStore(state => state.setWebSocket)

    const addLog = useCallback((text: string, type: 'stdout' | 'info' | 'error' | 'success' | 'status' = 'info') => {
        onLog?.(text, type)
    }, [onLog])

    const handleMessage = useCallback((event: MessageEvent) => {
        try {
            const data = JSON.parse(event.data)
            
            switch (data.event) {
                case 'init':
                    if (data.args?.length >= 3) {
                        setConnected(true)
                        setConnecting(false)
                        setStatus(data.args[2] as any)
                        addLog(`Connected to container`, 'success')
                        reconnectAttemptsRef.current = 0
                        
                        // Reset stats if stopped
                        if (['stopped', 'offline', 'exited'].includes(data.args[2])) {
                            setStats(null)
                        }
                    }
                    break
                    
                case 'console_output':
                    if (data.args?.[0]) {
                        addLog(data.args[0], 'stdout')
                    }
                    break
                    
                case 'status':
                    if (data.args?.[0]) {
                        const newStatus = data.args[0]
                        setStatus(newStatus)
                        addLog(`Status: ${newStatus}`, 'status')
                        
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
                        addLog(`[daemon] ${data.args[0]}`, 'info')
                    }
                    break
                    
                case 'error':
                    if (data.args?.[0]) {
                        addLog(`Error: ${data.args[0]}`, 'error')
                    }
                    break
                    
                default:
                    if (data.args?.length > 0) {
                        addLog(`[${data.event}] ${data.args.join(' ')}`, 'info')
                    }
            }
        } catch (e) {
            // If not JSON, treat as raw log output
            addLog(event.data, 'stdout')
        }
    }, [addLog, setConnected, setConnecting, setStatus, setStats, addStatsToHistory])

    const handleClose = useCallback((event: CloseEvent) => {
        setConnected(false)
        setConnecting(false)
        addLog(`Disconnected (code: ${event.code})`, 'info')
        
        // Attempt reconnection with exponential backoff
        if (reconnectAttemptsRef.current < maxReconnectAttempts) {
            const delay = Math.min(1000 * Math.pow(2, reconnectAttemptsRef.current), 10000)
            reconnectAttemptsRef.current++
            
            addLog(`Reconnecting in ${delay / 1000}s... (attempt ${reconnectAttemptsRef.current}/${maxReconnectAttempts})`, 'info')
            
            reconnectTimeoutRef.current = setTimeout(() => {
                connect()
            }, delay)
        } else {
            addLog('Max reconnection attempts reached', 'error')
        }
    }, [setConnected, setConnecting, addLog])

    const handleError = useCallback(() => {
        setConnected(false)
        setConnecting(false)
        addLog('Connection error', 'error')
    }, [setConnected, setConnecting, addLog])

    const connect = useCallback(async () => {
        if (!tenantId || !serverId) return
        
        // Check current state to prevent duplicate connections
        const currentState = useServerStore.getState()
        if (currentState.connecting || currentState.connected) return
        
        // Clear any pending reconnection
        if (reconnectTimeoutRef.current) {
            clearTimeout(reconnectTimeoutRef.current)
        }
        
        setConnecting(true)
        addLog('Generating token...', 'info')
        
        try {
            const credentials = await api.servers.websocket(tenantId, serverId)
            addLog('Connecting to console...', 'info')
            
            const newWs = new WebSocket(credentials.socket)
            
            newWs.onopen = () => {
                addLog('WebSocket connected, waiting for logs...', 'success')
            }
            
            newWs.onmessage = handleMessage
            newWs.onclose = handleClose
            newWs.onerror = handleError
            
            setWebSocket(newWs)
        } catch (e: any) {
            setConnecting(false)
            addLog(`Failed to connect: ${e?.message || 'Unknown error'}`, 'error')
        }
    }, [tenantId, serverId, api, handleMessage, handleClose, handleError, setConnecting, setWebSocket, addLog])

    const disconnect = useCallback(() => {
        if (reconnectTimeoutRef.current) {
            clearTimeout(reconnectTimeoutRef.current)
        }
        
        if (ws) {
            ws.close()
            setWebSocket(null)
        }
        
        setConnected(false)
        setConnecting(false)
        reconnectAttemptsRef.current = 0
    }, [ws, setWebSocket, setConnected, setConnecting])

    const sendCommand = useCallback((command: string) => {
        if (!ws || !connected || !command.trim()) return false
        
        try {
            ws.send(JSON.stringify({ event: 'send_command', args: [command.trim()] }))
            return true
        } catch (e) {
            addLog('Failed to send command', 'error')
            return false
        }
    }, [ws, connected, addLog])

    // Cleanup on unmount
    useEffect(() => {
        return () => {
            disconnect()
        }
    }, [])

    return {
        connect,
        disconnect,
        sendCommand,
        connected,
        connecting,
        status
    }
}
