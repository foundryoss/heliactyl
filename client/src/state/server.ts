import { create } from 'zustand'
import { immer } from 'zustand/middleware/immer'

export type ServerStatus = 'offline' | 'starting' | 'stopping' | 'running' | 'ready' | 'exited' | 'killed' | null

export interface ServerStats {
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

export interface ServerData {
    id: string
    name: string
    description?: string
    state: string
    containerId?: string
    nodeId?: string
    node?: string
    dockerImage?: string
    serverSoftwareId?: string
    memoryMb?: number
    diskMb?: number
    cpuPercent?: number
    limits?: {
        cpu?: string
        memory?: string
        disk?: string
        pids?: string
    }
    ports?: Array<{ hostPort: number; containerPort: number }>
    startup?: {
        command?: string
    }
}

interface ServerState {
    // Server data
    server: ServerData | null
    
    // Connection state
    connected: boolean
    connecting: boolean
    
    // Server status (from WebSocket)
    status: ServerStatus
    
    // Stats
    stats: ServerStats | null
    statsHistory: Array<{ time: string; memory: number; cpu: number }>
    
    // WebSocket instance
    ws: WebSocket | null
    
    // Computed states
    isRunning: boolean
    isOffline: boolean
    hasActiveResources: boolean
    
    // Actions
    setServer: (server: ServerData | null) => void
    setConnected: (connected: boolean) => void
    setConnecting: (connecting: boolean) => void
    setStatus: (status: ServerStatus) => void
    setStats: (stats: ServerStats | null) => void
    addStatsToHistory: (memory: number, cpu: number) => void
    clearStatsHistory: () => void
    setWebSocket: (ws: WebSocket | null) => void
    updateComputedStates: () => void
    reset: () => void
}

export const useServerStore = create<ServerState>()(
    immer((set, get) => ({
        // Initial state
        server: null,
        connected: false,
        connecting: false,
        status: null,
        stats: null,
        statsHistory: [],
        ws: null,
        isRunning: false,
        isOffline: true,
        hasActiveResources: false,

        // Actions
        setServer: (server) => set((state) => {
            state.server = server
        }),

        setConnected: (connected) => set((state) => {
            state.connected = connected
            
            // Update computed states inline
            const stats = state.stats
            const status = state.status
            state.hasActiveResources = !!(stats && (stats.cpu_absolute > 0 || stats.memory_bytes > 0))
            state.isRunning = (status === 'running' || status === 'ready') && 
                             (state.hasActiveResources || stats === null)
            state.isOffline = status === 'offline' || 
                             status === 'stopped' || 
                             status === 'exited' || 
                             status === 'killed' ||
                             ((status === 'running' || status === 'ready') && stats && !state.hasActiveResources)
        }),

        setConnecting: (connecting) => set((state) => {
            state.connecting = connecting
        }),

        setStatus: (status) => set((state) => {
            state.status = status
            
            // Reset stats when container stops
            if (['stopped', 'offline', 'exited', 'killed'].includes(status || '')) {
                state.stats = null
                // Add zero point to history
                const now = new Date().toLocaleTimeString('en-US', { 
                    hour12: false, 
                    hour: '2-digit', 
                    minute: '2-digit', 
                    second: '2-digit' 
                })
                state.statsHistory.push({ time: now, memory: 0, cpu: 0 })
                if (state.statsHistory.length > 30) {
                    state.statsHistory = state.statsHistory.slice(-30)
                }
            }
            
            // Update computed states inline
            const stats = state.stats
            state.hasActiveResources = !!(stats && (stats.cpu_absolute > 0 || stats.memory_bytes > 0))
            state.isRunning = (status === 'running' || status === 'ready') && 
                             (state.hasActiveResources || stats === null)
            state.isOffline = status === 'offline' || 
                             status === 'stopped' || 
                             status === 'exited' || 
                             status === 'killed' ||
                             ((status === 'running' || status === 'ready') && stats && !state.hasActiveResources)
        }),

        setStats: (stats) => set((state) => {
            state.stats = stats
            
            // Update computed states inline
            const status = state.status
            state.hasActiveResources = !!(stats && (stats.cpu_absolute > 0 || stats.memory_bytes > 0))
            state.isRunning = (status === 'running' || status === 'ready') && 
                             (state.hasActiveResources || stats === null)
            state.isOffline = status === 'offline' || 
                             status === 'stopped' || 
                             status === 'exited' || 
                             status === 'killed' ||
                             ((status === 'running' || status === 'ready') && stats && !state.hasActiveResources)
        }),

        addStatsToHistory: (memory, cpu) => set((state) => {
            const now = new Date().toLocaleTimeString('en-US', { 
                hour12: false, 
                hour: '2-digit', 
                minute: '2-digit', 
                second: '2-digit' 
            })
            state.statsHistory.push({ time: now, memory, cpu })
            if (state.statsHistory.length > 30) {
                state.statsHistory = state.statsHistory.slice(-30)
            }
        }),

        clearStatsHistory: () => set((state) => {
            state.statsHistory = []
        }),

        setWebSocket: (ws) => set((state) => {
            // Clean up old WebSocket
            if (state.ws && state.ws !== ws) {
                state.ws.close()
            }
            state.ws = ws
        }),

        reset: () => set((state) => {
            // Close WebSocket
            if (state.ws) {
                state.ws.close()
            }
            
            // Reset all state
            state.server = null
            state.connected = false
            state.connecting = false
            state.status = null
            state.stats = null
            state.statsHistory = []
            state.ws = null
            state.isRunning = false
            state.isOffline = true
            state.hasActiveResources = false
        }),
    }))
)
