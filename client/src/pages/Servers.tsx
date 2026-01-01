import React, { useCallback, useEffect, useState } from 'react'
import { useAuth } from '@/providers/AuthProvider'
import { useNavigate } from 'react-router-dom'
import { useApi } from '@/api/client'
import { useTenants } from '@/providers/TenantProvider'
import { useTenantUpdates } from '@/providers/MQTTProvider'
import { Card, CardHeader, CardTitle } from '@/components/ui/Card'
import { Button } from '@/components/ui/Button'
import { Input } from '@/components/ui/Input'
import { Label } from '@/components/ui/Label'
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
    const [nodes, setNodes] = useState<any[]>([])
    const [software, setSoftware] = useState<any[]>([])
    const [selectedSoftware, setSelectedSoftware] = useState<any>(null)
    const [resources, setResources] = useState<any>(null)
    const [error, setError] = useState<string | null>(null)
    const [loading, setLoading] = useState(false)
    const [showCreateModal, setShowCreateModal] = useState(false)
    const [creating, setCreating] = useState(false)
    const [showDeleteModal, setShowDeleteModal] = useState(false)
    const [serverToDelete, setServerToDelete] = useState<any>(null)
    const [deleting, setDeleting] = useState(false)
    
    // Form state - new structure
    const [formData, setFormData] = useState({
        name: '',
        description: '',
        serverSoftwareId: '',
        nodeId: '',
        dockerImage: '',
        env: {} as Record<string, string>
    })
    
    // RU estimate state
    const [ruEstimate, setRuEstimate] = useState<{
        ruPerHour: number
        ruPerDay: number
        ruPerMonth: number
        pricePerHour: number
        pricePerDay: number
        pricePerMonth: number
    } | null>(null)
    const [loadingEstimate, setLoadingEstimate] = useState(false)

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
    
    // Fetch RU estimate when node changes
    const fetchRuEstimate = useCallback(async (nodeId: string) => {
        if (!nodeId) {
            setRuEstimate(null)
            return
        }
        
        setLoadingEstimate(true)
        try {
            const estimate = await api.wallet.getRUEstimate(nodeId)
            console.log('RU estimate received:', estimate)
            setRuEstimate(estimate)
        } catch (e: any) {
            console.error('Failed to fetch RU estimate:', e)
            // Set a fallback estimate on error
            setRuEstimate({
                ruPerHour: 0.5,
                ruPerDay: 12.0,
                ruPerMonth: 360.0,
                pricePerHour: 0.0005,
                pricePerDay: 0.012,
                pricePerMonth: 0.36
            })
        } finally {
            setLoadingEstimate(false)
        }
    }, [api])

    const openCreateModal = async () => {
        if (!selectedTenantId) return
        
        try {
            // Load nodes and software
            const [nodesRes, softwareRes, resourcesRes] = await Promise.all([
                api.admin.nodes(),
                api.admin.software(),
                api.tenants.resources(selectedTenantId)
            ])
            
            setNodes(nodesRes.items || [])
            setSoftware(softwareRes.items || [])
            setResources(resourcesRes)
            
            // Reset form
            setFormData({
                name: '',
                description: '',
                serverSoftwareId: softwareRes.items?.[0]?.id || '',
                nodeId: nodesRes.items?.[0]?.id || '',
                dockerImage: '',
                env: {}
            })
            setRuEstimate(null)
            
            // Fetch RU estimate for initial node
            if (nodesRes.items?.[0]?.id) {
                fetchRuEstimate(nodesRes.items[0].id)
            }
            
            // Set initial software selection
            if (softwareRes.items?.[0]) {
                setSelectedSoftware(softwareRes.items[0])
                // Set default docker image
                const firstImage = Object.values(softwareRes.items[0].docker_images || {})[0]
                setFormData(prev => ({ ...prev, dockerImage: firstImage as string || '' }))
                
                // Set default env vars from software variables
                const defaultEnv: Record<string, string> = {}
                softwareRes.items[0].variables?.forEach((v: any) => {
                    defaultEnv[v.env_variable] = v.default_value
                })
                setFormData(prev => ({ ...prev, env: defaultEnv }))
            }
            
            setShowCreateModal(true)
        } catch (e: any) {
            console.error('Failed to load creation data:', e)
            notify({ type: 'error', description: 'Failed to load nodes and software' })
        }
    }
    
    const handleSoftwareChange = (softwareId: string) => {
        const sw = software.find(s => s.id === softwareId)
        setSelectedSoftware(sw)
        setFormData(prev => ({ ...prev, serverSoftwareId: softwareId }))
        
        if (sw) {
            // Set first docker image as default
            const firstImage = Object.values(sw.docker_images || {})[0]
            setFormData(prev => ({ ...prev, dockerImage: firstImage as string || '' }))
            
            // Set default env vars
            const defaultEnv: Record<string, string> = {}
            sw.variables?.forEach((v: any) => {
                defaultEnv[v.env_variable] = v.default_value
            })
            setFormData(prev => ({ ...prev, env: defaultEnv }))
        }
    }
    
    const handleNodeChange = (nodeId: string) => {
        setFormData(prev => ({ ...prev, nodeId }))
        fetchRuEstimate(nodeId)
    }

    useEffect(() => {
        loadData()
    }, [loadData])

    // Auto-refresh when there are servers being created (removed - now synchronous)

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
            const result = await api.servers.create(selectedTenantId, {
                name: formData.name,
                description: formData.description || undefined,
                serverSoftwareId: formData.serverSoftwareId,
                nodeId: formData.nodeId,
                env: formData.env
            })
            
            console.log('Server created:', result)
            
            notify({ 
                type: 'success', 
                title: 'Server Created',
                description: `Server "${result.name}" created successfully.` 
            })
            
            setShowCreateModal(false)
            setFormData({ name: '', description: '', serverSoftwareId: '', nodeId: '', dockerImage: '', env: {} })
            setSelectedSoftware(null)
            setRuEstimate(null)
            
            // Refresh server list
            loadData()
            
        } catch (e: any) {
            notify({ type: 'error', title: 'Creation Failed', description: e?.message || 'Failed to create server' })
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
                                        {server.description && (
                                            <div className="text-xs text-neutral-500 dark:text-neutral-400">
                                                {server.description}
                                            </div>
                                        )}
                                    </td>
                                    <td className="px-6 py-4 whitespace-nowrap">
                                        <div className="text-sm text-neutral-900 dark:text-neutral-100">
                                            {server.limits?.memory || 'N/A'}
                                        </div>
                                        <div className="text-xs text-neutral-500 dark:text-neutral-400">
                                            {server.limits?.disk || 'N/A'} • {server.limits?.cpu || 'N/A'}
                                        </div>
                                    </td>
                                    <td className="px-6 py-4 whitespace-nowrap">
                                        <span className={`inline-flex px-2 py-1 text-xs font-semibold rounded-full ${
                                            server.state === 'running' || server.state === 'ready'
                                                ? 'bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-400'
                                                : server.state === 'stopped' || server.state === 'exited'
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
                            placeholder="my-nodejs-server"
                            required
                        />
                    </div>

                    <div>
                        <Label htmlFor="description">Description (Optional)</Label>
                        <Input
                            id="description"
                            type="text"
                            value={formData.description}
                            onChange={(e) => setFormData({ ...formData, description: e.target.value })}
                            placeholder="My application server"
                        />
                    </div>

                    <div>
                        <Label htmlFor="serverSoftware">Server Software</Label>
                        <select
                            id="serverSoftware"
                            value={formData.serverSoftwareId}
                            onChange={(e) => handleSoftwareChange(e.target.value)}
                            className="w-full px-3 py-2 text-sm border border-neutral-200 dark:border-neutral-700 rounded-lg bg-white dark:bg-neutral-800"
                            required
                        >
                            <option value="">Select software...</option>
                            {software.map((sw) => (
                                <option key={sw.id} value={sw.id}>
                                    {sw.name}
                                </option>
                            ))}
                        </select>
                        <p className="text-xs text-neutral-500 dark:text-neutral-400 mt-1">
                            Choose the software stack for your server
                        </p>
                    </div>

                    {selectedSoftware && Object.keys(selectedSoftware.docker_images || {}).length > 0 && (
                        <div>
                            <Label htmlFor="dockerImage">Docker Image</Label>
                            <select
                                id="dockerImage"
                                value={formData.dockerImage}
                                onChange={(e) => setFormData({ ...formData, dockerImage: e.target.value })}
                                className="w-full px-3 py-2 text-sm border border-neutral-200 dark:border-neutral-700 rounded-lg bg-white dark:bg-neutral-800"
                                required
                            >
                                {Object.entries(selectedSoftware.docker_images).map(([key, value]) => (
                                    <option key={key} value={value as string}>
                                        {key}: {value as string}
                                    </option>
                                ))}
                            </select>
                            <p className="text-xs text-neutral-500 dark:text-neutral-400 mt-1">
                                Select which Docker image version to use
                            </p>
                        </div>
                    )}

                    {selectedSoftware && selectedSoftware.variables && selectedSoftware.variables.length > 0 && (
                        <div className="space-y-3">
                            <Label>Environment Variables</Label>
                            {selectedSoftware.variables.filter((v: any) => v.user_editable).map((variable: any) => (
                                <div key={variable.env_variable}>
                                    <Label htmlFor={variable.env_variable} className="text-xs">
                                        {variable.name}
                                        {variable.description && (
                                            <span className="text-neutral-500 dark:text-neutral-400 font-normal ml-1">
                                                - {variable.description}
                                            </span>
                                        )}
                                    </Label>
                                    <Input
                                        id={variable.env_variable}
                                        type={variable.field_type === 'number' ? 'number' : 'text'}
                                        value={formData.env[variable.env_variable] || variable.default_value}
                                        onChange={(e) => setFormData({
                                            ...formData,
                                            env: { ...formData.env, [variable.env_variable]: e.target.value }
                                        })}
                                        placeholder={variable.default_value}
                                        className="text-sm"
                                    />
                                </div>
                            ))}
                        </div>
                    )}

                    <div>
                        <Label htmlFor="node">Node</Label>
                        <select
                            id="node"
                            value={formData.nodeId}
                            onChange={(e) => handleNodeChange(e.target.value)}
                            className="w-full px-3 py-2 text-sm border border-neutral-200 dark:border-neutral-700 rounded-lg bg-white dark:bg-neutral-800"
                            required
                        >
                            <option value="">Select node...</option>
                            {nodes.map((node) => (
                                <option key={node.id} value={node.id}>
                                    {node.name} ({node.network.uri})
                                </option>
                            ))}
                        </select>
                        <p className="text-xs text-neutral-500 dark:text-neutral-400 mt-1">
                            Choose which node will host this server
                        </p>
                    </div>

                    {/* RU Cost Estimate */}
                    {formData.nodeId && (
                        <div className="bg-purple-50 dark:bg-purple-900/20 border border-purple-200 dark:border-purple-800 rounded-md p-3">
                            <p className="text-xs font-medium text-purple-900 dark:text-purple-100 mb-2">
                                Estimated Resource Usage
                            </p>
                            {loadingEstimate ? (
                                <p className="text-xs text-purple-700 dark:text-purple-300">Calculating...</p>
                            ) : ruEstimate ? (
                                <div className="space-y-1">
                                    <div className="flex justify-between text-xs">
                                        <span className="text-purple-700 dark:text-purple-300">Per Hour:</span>
                                        <span className="font-medium text-purple-900 dark:text-purple-100">
                                            {ruEstimate.ruPerHour.toFixed(2)} RU (${ruEstimate.pricePerHour.toFixed(4)})
                                        </span>
                                    </div>
                                    <div className="flex justify-between text-xs">
                                        <span className="text-purple-700 dark:text-purple-300">Per Day:</span>
                                        <span className="font-medium text-purple-900 dark:text-purple-100">
                                            {ruEstimate.ruPerDay.toFixed(2)} RU
                                        </span>
                                    </div>
                                    <div className="flex justify-between text-xs">
                                        <span className="text-purple-700 dark:text-purple-300">Per Month:</span>
                                        <span className="font-medium text-purple-900 dark:text-purple-100">
                                            {ruEstimate.ruPerMonth.toFixed(2)} RU (${ruEstimate.pricePerMonth.toFixed(2)})
                                        </span>
                                    </div>
                                </div>
                            ) : (
                                <p className="text-xs text-purple-700 dark:text-purple-300">Unable to calculate estimate</p>
                            )}
                        </div>
                    )}

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


