import { useState, useCallback } from 'react'
import { useAlert } from '@/components/ui/Alert'

export type PowerAction = 'start' | 'stop' | 'restart' | 'kill'

interface UseServerPowerOptions {
    tenantId: string | null
    serverId: string | null
    api: any
    onLog?: (text: string, type: 'info' | 'error') => void
}

export function useServerPower({ tenantId, serverId, api, onLog }: UseServerPowerOptions) {
    const [loading, setLoading] = useState<PowerAction | null>(null)
    const { notify } = useAlert()

    const sendPowerAction = useCallback(async (action: PowerAction) => {
        if (!tenantId || !serverId) return

        setLoading(action)

        const labels: Record<PowerAction, string> = {
            start: 'Starting',
            stop: 'Stopping',
            restart: 'Restarting',
            kill: 'Force stopping'
        }

        onLog?.(`${labels[action]} server...`, 'info')

        try {
            switch (action) {
                case 'start':
                    await api.servers.start(tenantId, serverId)
                    break
                case 'stop':
                    await api.servers.stop(tenantId, serverId)
                    break
                case 'restart':
                    await api.servers.restart(tenantId, serverId)
                    break
                case 'kill':
                    await api.servers.kill(tenantId, serverId)
                    break
            }

            notify({ 
                description: `${labels[action]} server...`, 
                type: 'success' 
            })
        } catch (e: any) {
            const errorMsg = e?.message || `Failed to ${action}`
            onLog?.(errorMsg, 'error')
            notify({ 
                description: errorMsg, 
                type: 'error' 
            })
        } finally {
            setLoading(null)
        }
    }, [api, tenantId, serverId, notify, onLog])

    return {
        sendPowerAction,
        loading
    }
}
