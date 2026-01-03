import { useEffect } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useServerStore } from '@/state/server'
import React from 'react'

interface UseServerOptions {
    tenantId: string | null
    serverId: string | null
    api: any
}

export function useServer({ tenantId, serverId, api }: UseServerOptions) {
    const queryClient = useQueryClient()
    const setServer = useServerStore(state => state.setServer)

    // Fetch server data with caching
    const { data: server, isLoading, error, refetch } = useQuery({
        queryKey: ['server', tenantId, serverId],
        queryFn: async () => {
            if (!tenantId || !serverId) return null
            
            const serversRes = await api.servers.list(tenantId)
            const foundServer = serversRes.items.find((s: any) => s.id === serverId)
            
            if (!foundServer) {
                throw new Error('Server not found')
            }
            
            return foundServer
        },
        enabled: !!tenantId && !!serverId,
        staleTime: 30000, // Consider data fresh for 30 seconds
        gcTime: 5 * 60 * 1000, // Keep in cache for 5 minutes
        refetchOnWindowFocus: true, // Refetch when user comes back to tab
        refetchInterval: 60000, // Background refetch every minute
    })

    // Update Zustand store when server data changes
    useEffect(() => {
        if (server) {
            setServer(server)
        }
    }, [server, setServer])

    // Mutation for updating server settings
    const updateServerMutation = useMutation({
        mutationFn: async (updates: Partial<any>) => {
            if (!tenantId || !serverId) throw new Error('Missing tenant or server ID')
            
            // Call your update API here
            const response = await api.servers.update(tenantId, serverId, updates)
            return response
        },
        onMutate: async (updates) => {
            // Cancel outgoing refetches
            await queryClient.cancelQueries({ queryKey: ['server', tenantId, serverId] })
            
            // Snapshot previous value
            const previousServer = queryClient.getQueryData(['server', tenantId, serverId])
            
            // Optimistically update cache
            queryClient.setQueryData(['server', tenantId, serverId], (old: any) => ({
                ...old,
                ...updates
            }))
            
            // Update Zustand store immediately for instant UI feedback
            if (previousServer) {
                setServer({ ...previousServer as any, ...updates })
            }
            
            return { previousServer }
        },
        onError: (err, updates, context) => {
            // Rollback on error
            if (context?.previousServer) {
                queryClient.setQueryData(['server', tenantId, serverId], context.previousServer)
                setServer(context.previousServer as any)
            }
        },
        onSettled: () => {
            // Refetch to ensure we have the latest data
            queryClient.invalidateQueries({ queryKey: ['server', tenantId, serverId] })
        }
    })

    return {
        server,
        isLoading,
        error,
        refetch,
        updateServer: updateServerMutation.mutate,
        isUpdating: updateServerMutation.isPending
    }
}
