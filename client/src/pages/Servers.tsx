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
import { PlusIcon, TrashIcon, CommandLineIcon, WifiIcon } from '@heroicons/react/24/outline'
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
    const [locations, setLocations] = useState<any[]>([])
    const [eggs, setEggs] = useState<any[]>([])
    const [resources, setResources] = useState<any>(null)
    const [error, setError] = useState<string | null>(null)
    const [loading, setLoading] = useState(false)
    const [showCreateModal, setShowCreateModal] = useState(false)
    const [creating, setCreating] = useState(false)
    const [showDeleteModal, setShowDeleteModal] = useState(false)
    const [serverToDelete, setServerToDelete] = useState<any>(null)
    const [deleting, setDeleting] = useState(false)
    
    // Form state
    const [formData, setFormData] = useState({
        name: '',
        eggId: '',
        memoryMb: '',
        diskMb: '',
        cpuPercent: '',
        location: ''
    })

    const loadData = useCallback(async () => {
        if (!selectedTenantId) return
        setLoading(true)
        setError(null)
        
        try {
            const [serversRes, locationsRes, eggsRes] = await Promise.all([
                api.servers.list(selectedTenantId),
                api.core.locations(),
                api.core.eggs()
            ])
            
            setServers(serversRes.items || [])
            setLocations(locationsRes.items || [])
            setEggs(eggsRes.items || [])
        } catch (e: any) {
            setError(e?.message || 'Failed to load data')
        } finally {
            setLoading(false)
        }
    }, [api, selectedTenantId])

    const openCreateModal = async () => {
        console.log('Opening create modal...')
        if (!selectedTenantId) return
        
        try {
            const resourcesRes = await api.tenants.resources(selectedTenantId)
            setResources(resourcesRes)
            
            // Auto-fill with available resources
            setFormData({
                name: '',
                eggId: eggs.length > 0 ? eggs[0].eggId.toString() : '',
                memoryMb: resourcesRes.remaining?.memoryMb?.toString() || '1024',
                diskMb: resourcesRes.remaining?.diskMb?.toString() || '2048',
                cpuPercent: resourcesRes.remaining?.cpuPercent?.toString() || '100',
                location: locations.length > 0 ? locations[0].slug : ''
            })
            
            setShowCreateModal(true)
        } catch (e: any) {
            console.error('Failed to load resources:', e)
            // Still show modal even if resources fail to load
            setFormData({
                name: '',
                eggId: eggs.length > 0 ? eggs[0].eggId.toString() : '',
                memoryMb: '1024',
                diskMb: '2048',
                cpuPercent: '100',
                location: locations.length > 0 ? locations[0].slug : ''
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
                    description: `Server "${latestUpdate.data.name}" was created`,
                    icon: <WifiIcon className="w-4 h-4" />
                })
                loadData() // Refresh server list
                break
                
            case 'server_deleted':
                notify({ 
                    type: 'info', 
                    description: `A server was deleted`,
                    icon: <WifiIcon className="w-4 h-4" />
                })
                loadData() // Refresh server list
                break
                
            case 'member_added':
            case 'member_removed':
                notify({ 
                    type: 'info', 
                    description: `Team membership was updated`,
                    icon: <WifiIcon className="w-4 h-4" />
                })
                break
        }
    }, [liveUpdates, notify, loadData])

    const handleCreateServer = async (e: React.FormEvent) => {
        e.preventDefault()
        if (!selectedTenantId) return
        
        setCreating(true)
        try {
            await api.servers.create(selectedTenantId, {
                name: formData.name,
                eggId: Number(formData.eggId),
                memoryMb: Number(formData.memoryMb),
                diskMb: Number(formData.diskMb),
                cpuPercent: Number(formData.cpuPercent),
                location: formData.location
            })
            
            notify({ type: 'success', description: 'Server created successfully' })
            setShowCreateModal(false)
            setFormData({ name: '', eggId: '', memoryMb: '', diskMb: '', cpuPercent: '', location: '' })
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
                                    Egg
                                </th>
                                <th className="px-6 py-3 text-left text-xs font-medium text-neutral-500 dark:text-neutral-400 uppercase tracking-wider">
                                    Resources
                                </th>
                                <th className="px-6 py-3 text-left text-xs font-medium text-neutral-500 dark:text-neutral-400 uppercase tracking-wider">
                                    Location
                                </th>
                                <th className="px-6 py-3 text-left text-xs font-medium text-neutral-500 dark:text-neutral-400 uppercase tracking-wider">
                                    Created
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
                                        <div className="text-sm text-neutral-500 dark:text-neutral-400">
                                            ID: {server.pteroServerId}
                                        </div>
                                    </td>
                                    <td className="px-6 py-4 whitespace-nowrap">
                                        <div className="text-sm text-neutral-900 dark:text-neutral-100">
                                            {eggs.find(e => e.eggId === server.eggId)?.name || `Egg ${server.eggId}`}
                                        </div>
                                        <div className="text-sm text-neutral-500 dark:text-neutral-400">
                                            {server.dockerImage}
                                        </div>
                                    </td>
                                    <td className="px-6 py-4 whitespace-nowrap">
                                        <div className="text-sm text-neutral-900 dark:text-neutral-100">
                                            {server.memoryMb}MB RAM
                                        </div>
                                        <div className="text-sm text-neutral-500 dark:text-neutral-400">
                                            {server.diskMb}MB Disk • {server.cpuPercent}% CPU
                                        </div>
                                    </td>
                                    <td className="px-6 py-4 whitespace-nowrap">
                                        <div className="text-sm text-neutral-900 dark:text-neutral-100">
                                            {locations.find(l => l.id === server.locationId)?.name || `Location ${server.locationId}`}
                                        </div>
                                    </td>
                                    <td className="px-6 py-4 whitespace-nowrap text-sm text-neutral-500 dark:text-neutral-400">
                                        {new Date(server.createdAt).toLocaleDateString()}
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
                            placeholder="My Server"
                            required
                        />
                    </div>

                    <div>
                        <Label htmlFor="eggId">Egg</Label>
                        <Select
                            id="eggId"
                            value={formData.eggId}
                            onChange={(value) => setFormData({ ...formData, eggId: value })}
                            options={[
                                { value: '', label: 'Select an egg' },
                                ...eggs.map((egg) => ({
                                    value: egg.eggId.toString(),
                                    label: `${egg.name} (${egg.dockerImage})`
                                }))
                            ]}
                            placeholder="Select an egg"
                            required
                        />
                    </div>

                    <div>
                        <Label htmlFor="location">Location</Label>
                        <Select
                            id="location"
                            value={formData.location}
                            onChange={(value) => setFormData({ ...formData, location: value })}
                            options={[
                                { value: '', label: 'Select a location' },
                                ...locations.map((location) => ({
                                    value: location.slug,
                                    label: location.name
                                }))
                            ]}
                            placeholder="Select a location"
                            required
                        />
                    </div>

                    <div className="grid grid-cols-3 gap-4">
                        <div>
                            <Label htmlFor="memoryMb">Memory (MB)</Label>
                            <Input
                                id="memoryMb"
                                type="number"
                                value={formData.memoryMb}
                                onChange={(e) => setFormData({ ...formData, memoryMb: e.target.value })}
                                placeholder="1024"
                                required
                            />
                        </div>

                        <div>
                            <Label htmlFor="diskMb">Disk (MB)</Label>
                            <Input
                                id="diskMb"
                                type="number"
                                value={formData.diskMb}
                                onChange={(e) => setFormData({ ...formData, diskMb: e.target.value })}
                                placeholder="2048"
                                required
                            />
                        </div>

                        <div>
                            <Label htmlFor="cpuPercent">CPU (%)</Label>
                            <Input
                                id="cpuPercent"
                                type="number"
                                value={formData.cpuPercent}
                                onChange={(e) => setFormData({ ...formData, cpuPercent: e.target.value })}
                                placeholder="100"
                                required
                            />
                        </div>
                    </div>

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


