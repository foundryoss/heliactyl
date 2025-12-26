import React, { useCallback, useEffect, useState } from 'react'
import { useAuth } from '@/providers/AuthProvider'
import { useNavigate } from 'react-router-dom'
import { useApi } from '@/api/client'
import { useTenants } from '@/providers/TenantProvider'
import { useTenantUpdates } from '@/providers/MQTTProvider'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card'
import { Button } from '@/components/ui/Button'
import { Input } from '@/components/ui/Input'
import { Label } from '@/components/ui/Label'
import { Select } from '@/components/ui/Select'
import { Modal } from '@/components/ui/Modal'
import Spinner from '@/components/ui/Spinner'
import { PlusIcon, TrashIcon, CommandLineIcon } from '@heroicons/react/24/outline'
import { useAlert } from '@/components/ui/Alert'

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
    const [resources, setResources] = useState<any>(null)
    const [error, setError] = useState<string | null>(null)
    const [loading, setLoading] = useState(false)
    const [showCreateModal, setShowCreateModal] = useState(false)
    const [creating, setCreating] = useState(false)
    const [showDeleteModal, setShowDeleteModal] = useState(false)
    const [serverToDelete, setServerToDelete] = useState<any>(null)
    const [deleting, setDeleting] = useState(false)
    
    // Form state - simplified for daemon
    const [formData, setFormData] = useState({
        name: '',
        dockerImage: 'nginx:alpine',
        memoryMb: '512',
        diskMb: '1024',
        cpuPercent: '100'
    })

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

    const openCreateModal = async () => {
        if (!selectedTenantId) return
        
        try {
            const resourcesRes = await api.tenants.resources(selectedTenantId)
            setResources(resourcesRes)
            
            // Auto-fill with available resources
            setFormData({
                name: '',
                dockerImage: 'nginx:alpine',
                memoryMb: Math.min(resourcesRes.remaining?.memoryMb || 512, 512).toString(),
                diskMb: Math.min(resourcesRes.remaining?.diskMb || 1024, 1024).toString(),
                cpuPercent: Math.min(resourcesRes.remaining?.cpuPercent || 100, 100).toString()
            })
            
            setShowCreateModal(true)
        } catch (e: any) {
            console.error('Failed to load resources:', e)
            setFormData({
                name: '',
                dockerImage: 'nginx:alpine',
                memoryMb: '512',
                diskMb: '1024',
                cpuPercent: '100'
            })
            setShowCreateModal(true)
        }
    }

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
                loadData() // Refresh server list
                break
                
            case 'server_deleted':
                notify({ 
                    type: 'info', 
                    description: `A server was deleted`
                })
                loadData() // Refresh server list
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

    const handleCreateServer = async (e: React.FormEvent) => {
        e.preventDefault()
        if (!selectedTenantId) return
        
        setCreating(true)
        
        try {
            const response = await fetch(`/api/tenants/${selectedTenantId}/servers`, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                    'Authorization': `Bearer ${token}`
                },
                body: JSON.stringify({
                    name: formData.name,
                    dockerImage: formData.dockerImage,
                    memoryMb: Number(formData.memoryMb),
                    diskMb: Number(formData.diskMb),
                    cpuPercent: Number(formData.cpuPercent),
                    env: {}
                })
            })
            
            if (!response.ok) {
                const errorData = await response.json().catch(() => ({ error: 'Failed to create server' }))
                throw new Error(errorData.error || 'Failed to create server')
            }
            
            const result = await response.json()
            console.log('Server creation started:', result)
            
            notify({ 
                type: 'success', 
                description: `Server "${result.name}" is being created. It will appear when ready.` 
            })
            
            setShowCreateModal(false)
            setFormData({ name: '', dockerImage: 'nginx:alpine', memoryMb: '512', diskMb: '1024', cpuPercent: '100' })
            
            // Refresh server list to show "creating" state
            loadData()
            
        } catch (e: any) {
            notify({ type: 'error', description: e?.message || 'Failed to create server' })
        } finally {
            setCreating(false)
        }
    }


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
        <Card><CardHeader><CardTitle>Select a tenant</CardTitle></CardHeader></Card>
    )

    if (loading) return (
        <div className="flex items-center justify-center py-12"><Spinner size="lg" /></div>
    )

    if (error) return (
        <Card><CardHeader><CardTitle className="text-red-600">{error}</CardTitle></CardHeader></Card>
    )

    return (
        <div className="space-y-6">
            <div className="flex items-center justify-between">
                <div>
                    <h1 className="text-2xl font-semibold text-neutral-900 dark:text-neutral-100">Servers</h1>
                    <p className="text-sm text-neutral-600 dark:text-neutral-400">Manage servers for the selected tenant</p>
                </div>
                <Button onClick={openCreateModal} className="flex items-center gap-2 shrink-0">
                    <PlusIcon className="w-4 h-4" />
                    Create Server
                </Button>
            </div>

            <Card>
                <div className="overflow-hidden">
                    <table className="min-w-full divide-y divide-neutral-200 dark:divide-neutral-700">
                        <thead className="bg-neutral-50 dark:bg-neutral-800/50">
                            <tr>
                                <th className="px-6 py-3 text-left text-xs font-medium text-neutral-500 dark:text-neutral-400 uppercase tracking-wider">
                                    Name
                                </th>
                                <th className="px-6 py-3 text-left text-xs font-medium text-neutral-500 dark:text-neutral-400 uppercase tracking-wider">
                                    Image
                                </th>
                                <th className="px-6 py-3 text-left text-xs font-medium text-neutral-500 dark:text-neutral-400 uppercase tracking-wider">
                                    Resources
                                </th>
                                <th className="px-6 py-3 text-left text-xs font-medium text-neutral-500 dark:text-neutral-400 uppercase tracking-wider">
                                    State
                                </th>
                                <th className="px-6 py-3 text-left text-xs font-medium text-neutral-500 dark:text-neutral-400 uppercase tracking-wider">
                                    Ports
                                </th>
                                <th className="relative px-6 py-3">
                                    <span className="sr-only">Actions</span>
                                </th>
                            </tr>
                        </thead>
                        <tbody className="bg-white dark:bg-neutral-900/50 divide-y divide-neutral-200 dark:divide-neutral-700">
                            {servers.map((server) => (
                                <tr key={server.id} className="hover:bg-neutral-50 dark:hover:bg-neutral-800/50">
                                    <td className="px-6 py-4 whitespace-nowrap">
                                        <div className="text-sm font-medium text-neutral-900 dark:text-neutral-100">
                                            {server.name}
                                        </div>
                                        <div className="text-xs text-neutral-500 dark:text-neutral-400">
                                            {server.containerId?.substring(0, 12) || 'N/A'}
                                        </div>
                                    </td>
                                    <td className="px-6 py-4">
                                        <div className="text-sm text-neutral-900 dark:text-neutral-100">
                                            {server.dockerImage}
                                        </div>
                                    </td>
                                    <td className="px-6 py-4 whitespace-nowrap">
                                        <div className="text-sm text-neutral-900 dark:text-neutral-100">
                                            {server.memoryMb}MB RAM
                                        </div>
                                        <div className="text-xs text-neutral-500 dark:text-neutral-400">
                                            {server.diskMb}MB Disk • {server.cpuPercent}% CPU
                                        </div>
                                    </td>
                                    <td className="px-6 py-4 whitespace-nowrap">
                                        <span className={`inline-flex px-2 py-1 text-xs font-semibold rounded-full ${
                                            server.state === 'running' 
                                                ? 'bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-400'
                                                : server.state === 'stopped'
                                                ? 'bg-red-100 text-red-800 dark:bg-red-900/30 dark:text-red-400'
                                                : 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900/30 dark:text-yellow-400'
                                        }`}>
                                            {server.state || 'unknown'}
                                        </span>
                                    </td>
                                    <td className="px-6 py-4 whitespace-nowrap">
                                        <div className="text-sm text-neutral-900 dark:text-neutral-100">
                                            {server.ports?.map((p: any) => p.hostPort).join(', ') || 'None'}
                                        </div>
                                    </td>
                                    <td className="px-6 py-4 whitespace-nowrap text-right text-sm font-medium">
                                        <div className="flex items-center gap-2 justify-end">
                                            <Button
                                                variant="ghost"
                                                size="sm"
                                                onClick={() => navigate(`/server/${server.id}`)}
                                                className="text-blue-600 hover:text-blue-900 dark:text-blue-400 dark:hover:text-blue-300"
                                            >
                                                <CommandLineIcon className="w-4 h-4" />
                                            </Button>
                                            <Button
                                                variant="ghost"
                                                size="sm"
                                                onClick={() => handleDeleteClick(server)}
                                                className="text-red-600 hover:text-red-900 dark:text-red-400 dark:hover:text-red-300"
                                            >
                                                <TrashIcon className="w-4 h-4" />
                                            </Button>
                                        </div>
                                    </td>
                                </tr>
                            ))}
                        </tbody>
                    </table>
                    
                    {servers.length === 0 && (
                        <div className="text-center py-12">
                            <p className="text-sm text-neutral-500 dark:text-neutral-400">No servers found</p>
                            <p className="text-xs text-neutral-400 dark:text-neutral-500 mt-1">Create your first server to get started</p>
                        </div>
                    )}
                </div>
            </Card>

            <Modal 
                open={showCreateModal} 
                onClose={() => setShowCreateModal(false)}
                title="Create Server"
            >
                <form onSubmit={handleCreateServer} className="space-y-4">
                    <div>
                        <Label htmlFor="name">Server Name</Label>
                        <Input
                            id="name"
                            type="text"
                            value={formData.name}
                            onChange={(e) => setFormData({ ...formData, name: e.target.value })}
                            placeholder="my-server"
                            required
                        />
                    </div>

                    <div>
                        <Label htmlFor="dockerImage">Docker Image</Label>
                        <Input
                            id="dockerImage"
                            type="text"
                            value={formData.dockerImage}
                            onChange={(e) => setFormData({ ...formData, dockerImage: e.target.value })}
                            placeholder="nginx:alpine"
                            required
                        />
                        <p className="text-xs text-neutral-500 dark:text-neutral-400 mt-1">
                            Examples: nginx:alpine, node:18-alpine, python:3.11-slim
                        </p>
                    </div>

                    <div className="grid grid-cols-3 gap-4">
                        <div>
                            <Label htmlFor="memoryMb">Memory (MB)</Label>
                            <Input
                                id="memoryMb"
                                type="number"
                                min="128"
                                value={formData.memoryMb}
                                onChange={(e) => setFormData({ ...formData, memoryMb: e.target.value })}
                                placeholder="512"
                                required
                            />
                        </div>

                        <div>
                            <Label htmlFor="diskMb">Disk (MB)</Label>
                            <Input
                                id="diskMb"
                                type="number"
                                min="256"
                                value={formData.diskMb}
                                onChange={(e) => setFormData({ ...formData, diskMb: e.target.value })}
                                placeholder="1024"
                                required
                            />
                        </div>

                        <div>
                            <Label htmlFor="cpuPercent">CPU (%)</Label>
                            <Input
                                id="cpuPercent"
                                type="number"
                                min="10"
                                max="400"
                                value={formData.cpuPercent}
                                onChange={(e) => setFormData({ ...formData, cpuPercent: e.target.value })}
                                placeholder="100"
                                required
                            />
                        </div>
                    </div>

                    {resources && (
                        <div className="bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded-md p-3">
                            <p className="text-xs font-medium text-blue-900 dark:text-blue-100 mb-1">Available Resources</p>
                            <p className="text-xs text-blue-700 dark:text-blue-300">
                                RAM: {resources.remaining?.memoryMb || 0}MB • 
                                Disk: {resources.remaining?.diskMb || 0}MB • 
                                CPU: {resources.remaining?.cpuPercent || 0}%
                            </p>
                        </div>
                    )}

                    <div className="flex justify-end gap-2 pt-4">
                        <Button
                            type="button"
                            variant="ghost"
                            onClick={() => setShowCreateModal(false)}
                            disabled={creating}
                            className="border-0"
                        >
                            Cancel
                        </Button>
                        <Button type="submit" isLoading={creating}>
                            Create Server
                        </Button>
                    </div>
                </form>
            </Modal>

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


