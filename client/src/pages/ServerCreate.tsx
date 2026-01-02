import React, { useCallback, useEffect, useState } from 'react'
import { useAuth } from '@/providers/AuthProvider'
import { useNavigate } from 'react-router-dom'
import { useApi } from '@/api/client'
import { useTenants } from '@/providers/TenantProvider'
import { Button } from '@/components/ui/Button'
import { Input } from '@/components/ui/Input'
import { Label } from '@/components/ui/Label'
import Spinner from '@/components/ui/Spinner'
import { useAlert } from '@/components/ui/Alert'
import { ArrowLeftIcon } from '@heroicons/react/24/outline'

interface ServerSoftware {
    id: string
    name: string
    icon_url?: string
    docker_images: Record<string, string>
    startup_cmd: string
    variables: Array<{
        name: string
        description: string
        env_variable: string
        default_value: string
        user_viewable: boolean
        user_editable: boolean
        field_type: string
    }>
}

interface Node {
    id: string
    name: string
    network: {
        uri: string
        scheme: string
    }
}

export function ServerCreatePage() {
    const { token } = useAuth()
    const getTokenFn = useCallback(() => token, [token])
    const api = useApi(getTokenFn)
    const { selectedTenantId } = useTenants()
    const { notify } = useAlert()
    const navigate = useNavigate()

    const [loading, setLoading] = useState(true)
    const [software, setSoftware] = useState<ServerSoftware[]>([])
    const [nodes, setNodes] = useState<Node[]>([])
    const [selectedSoftware, setSelectedSoftware] = useState<ServerSoftware | null>(null)

    // Form state
    const [formData, setFormData] = useState({
        name: '',
        description: '',
        serverSoftwareId: '',
        nodeId: '',
        dockerImage: '',
        env: {} as Record<string, string>,
        limits: {
            memory: '512',
            disk: '5',
            cpu: '50'
        }
    })

    // Load software and nodes
    useEffect(() => {
        const loadData = async () => {
            try {
                const [softwareRes, nodesRes] = await Promise.all([
                    api.admin.software(),
                    api.admin.nodes()
                ])
                setSoftware(softwareRes.items || [])
                setNodes(nodesRes.items || [])

                // Set defaults
                if (softwareRes.items?.[0]) {
                    handleSoftwareChange(softwareRes.items[0].id, softwareRes.items)
                }
                if (nodesRes.items?.[0]) {
                    setFormData(prev => ({ ...prev, nodeId: nodesRes.items[0].id }))
                }
            } catch (e: any) {
                notify({ type: 'error', description: 'Failed to load data' })
            } finally {
                setLoading(false)
            }
        }
        loadData()
    }, [api])

    const handleSoftwareChange = (softwareId: string, softwareList?: ServerSoftware[]) => {
        const list = softwareList || software
        const sw = list.find(s => s.id === softwareId)
        setSelectedSoftware(sw || null)
        setFormData(prev => ({ ...prev, serverSoftwareId: softwareId }))

        if (sw) {
            // Set first docker image
            const firstImage = Object.values(sw.docker_images || {})[0]
            setFormData(prev => ({ ...prev, dockerImage: firstImage || '' }))

            // Set default env vars
            const defaultEnv: Record<string, string> = {}
            sw.variables?.forEach((v) => {
                defaultEnv[v.env_variable] = v.default_value
            })
            setFormData(prev => ({ ...prev, env: defaultEnv }))
        }
    }

    const handleCreate = async (e: React.FormEvent) => {
        e.preventDefault()
        if (!selectedTenantId) return

        // Navigate to preflight page with form data
        navigate('/servers/create/preflight', {
            state: { formData }
        })
    }

    if (!selectedTenantId) {
        return (
            <div className="bg-neutral-100 dark:bg-neutral-900/30 border border-neutral-300 dark:border-neutral-800/50 p-6">
                <p className="text-sm text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                    Select a tenant first
                </p>
            </div>
        )
    }

    if (loading) {
        return (
            <div className="flex items-center justify-center py-12">
                <Spinner size="lg" />
            </div>
        )
    }

    return (
        <div className="space-y-6">
            {/* Header */}
            <div className="flex items-center gap-4">
                <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => navigate('/servers')}
                    className="text-neutral-600 dark:text-neutral-400 hover:text-neutral-900 dark:hover:text-neutral-100"
                >
                    <ArrowLeftIcon className="w-4 h-4" />
                </Button>
                <h1
                    className="text-2xl text-neutral-900 dark:text-neutral-100 tracking-wider"
                    style={{ fontFamily: "'Seven Segment', sans-serif" }}
                >
                    CREATE SERVER
                </h1>
                <div className="flex-1 h-px bg-neutral-300 dark:bg-neutral-800"></div>
            </div>

            <form onSubmit={handleCreate} className="space-y-6">
                {/* Basic Info */}
                <div className="bg-neutral-100 dark:bg-neutral-900/30 border border-neutral-300 dark:border-neutral-800/50 p-6">
                    <div className="flex items-center gap-3 mb-6">
                        <h2
                            className="text-lg text-neutral-900 dark:text-neutral-100 tracking-wider"
                            style={{ fontFamily: "'Seven Segment', sans-serif" }}
                        >
                            BASIC INFO
                        </h2>
                        <div className="flex-1 h-px bg-neutral-300 dark:bg-neutral-800"></div>
                    </div>

                    <div className="grid md:grid-cols-2 gap-4">
                        <div>
                            <Label htmlFor="name" className="text-[10px] uppercase tracking-[0.2em] text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                                Server Name
                            </Label>
                            <Input
                                id="name"
                                value={formData.name}
                                onChange={(e) => setFormData({ ...formData, name: e.target.value })}
                                placeholder="my-server"
                                required
                                className="bg-neutral-50 dark:bg-neutral-900/50 border-neutral-300 dark:border-neutral-800/50"
                            />
                        </div>
                        <div>
                            <Label htmlFor="description" className="text-[10px] uppercase tracking-[0.2em] text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                                Description
                            </Label>
                            <Input
                                id="description"
                                value={formData.description}
                                onChange={(e) => setFormData({ ...formData, description: e.target.value })}
                                placeholder="Optional description"
                                className="bg-neutral-50 dark:bg-neutral-900/50 border-neutral-300 dark:border-neutral-800/50"
                            />
                        </div>
                    </div>
                </div>

                {/* Software Selection */}
                <div className="bg-neutral-100 dark:bg-neutral-900/30 border border-neutral-300 dark:border-neutral-800/50 p-6">
                    <div className="flex items-center gap-3 mb-6">
                        <h2
                            className="text-lg text-neutral-900 dark:text-neutral-100 tracking-wider"
                            style={{ fontFamily: "'Seven Segment', sans-serif" }}
                        >
                            SOFTWARE
                        </h2>
                        <div className="flex-1 h-px bg-neutral-300 dark:bg-neutral-800"></div>
                    </div>

                    <div className="grid md:grid-cols-2 gap-4">
                        <div>
                            <Label className="text-[10px] uppercase tracking-[0.2em] text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                                Server Software
                            </Label>
                            <select
                                value={formData.serverSoftwareId}
                                onChange={(e) => handleSoftwareChange(e.target.value)}
                                className="w-full px-3 py-2 text-sm bg-neutral-50 dark:bg-neutral-900/50 border border-neutral-300 dark:border-neutral-800/50 text-neutral-900 dark:text-neutral-100"
                                style={{ fontFamily: "'Space Mono', monospace" }}
                                required
                            >
                                <option value="">Select software...</option>
                                {software.map((sw) => (
                                    <option key={sw.id} value={sw.id}>{sw.name}</option>
                                ))}
                            </select>
                        </div>

                        {selectedSoftware && Object.keys(selectedSoftware.docker_images || {}).length > 0 && (
                            <div>
                                <Label className="text-[10px] uppercase tracking-[0.2em] text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                                    Docker Image
                                </Label>
                                <select
                                    value={formData.dockerImage}
                                    onChange={(e) => setFormData({ ...formData, dockerImage: e.target.value })}
                                    className="w-full px-3 py-2 text-sm bg-neutral-50 dark:bg-neutral-900/50 border border-neutral-300 dark:border-neutral-800/50 text-neutral-900 dark:text-neutral-100"
                                    style={{ fontFamily: "'Space Mono', monospace" }}
                                    required
                                >
                                    {Object.entries(selectedSoftware.docker_images).map(([key, value]) => (
                                        <option key={key} value={value}>{key}: {value}</option>
                                    ))}
                                </select>
                            </div>
                        )}
                    </div>

                    {/* Environment Variables */}
                    {selectedSoftware?.variables?.filter(v => v.user_editable).length > 0 && (
                        <div className="mt-6">
                            <Label className="text-[10px] uppercase tracking-[0.2em] text-neutral-600 dark:text-neutral-500 mb-3 block" style={{ fontFamily: "'Space Mono', monospace" }}>
                                Environment Variables
                            </Label>
                            <div className="grid md:grid-cols-2 gap-4">
                                {selectedSoftware.variables.filter(v => v.user_editable).map((variable) => (
                                    <div key={variable.env_variable}>
                                        <Label className="text-xs text-neutral-700 dark:text-neutral-400" style={{ fontFamily: "'Space Mono', monospace" }}>
                                            {variable.name}
                                            {variable.description && (
                                                <span className="text-neutral-500 dark:text-neutral-600 ml-1">- {variable.description}</span>
                                            )}
                                        </Label>
                                        <Input
                                            value={formData.env[variable.env_variable] || variable.default_value}
                                            onChange={(e) => setFormData({
                                                ...formData,
                                                env: { ...formData.env, [variable.env_variable]: e.target.value }
                                            })}
                                            placeholder={variable.default_value}
                                            className="bg-neutral-50 dark:bg-neutral-900/50 border-neutral-300 dark:border-neutral-800/50"
                                        />
                                    </div>
                                ))}
                            </div>
                        </div>
                    )}
                </div>

                {/* Resource Limits */}
                <div className="bg-neutral-100 dark:bg-neutral-900/30 border border-neutral-300 dark:border-neutral-800/50 p-6">
                    <div className="flex items-center gap-3 mb-6">
                        <h2
                            className="text-lg text-neutral-900 dark:text-neutral-100 tracking-wider"
                            style={{ fontFamily: "'Seven Segment', sans-serif" }}
                        >
                            RESOURCE LIMITS
                        </h2>
                        <div className="flex-1 h-px bg-neutral-300 dark:bg-neutral-800"></div>
                    </div>

                    <div className="grid md:grid-cols-3 gap-6">
                        <div>
                            <Label className="text-[10px] uppercase tracking-[0.2em] text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                                Memory (MB)
                            </Label>
                            <Input
                                type="number"
                                value={formData.limits.memory}
                                onChange={(e) => setFormData({
                                    ...formData,
                                    limits: { ...formData.limits, memory: e.target.value }
                                })}
                                min="128"
                                max="8192"
                                className="bg-neutral-50 dark:bg-neutral-900/50 border-neutral-300 dark:border-neutral-800/50"
                            />
                            <div className="mt-2 flex gap-2">
                                {['256', '512', '1024', '2048'].map(val => (
                                    <button
                                        key={val}
                                        type="button"
                                        onClick={() => setFormData({ ...formData, limits: { ...formData.limits, memory: val } })}
                                        className={`px-2 py-1 text-xs border transition-colors ${formData.limits.memory === val
                                            ? 'bg-red-500/20 border-red-500/50 text-red-600 dark:text-red-400'
                                            : 'bg-neutral-50 dark:bg-neutral-900/50 border-neutral-300 dark:border-neutral-800/50 text-neutral-600 dark:text-neutral-400 hover:border-neutral-400 dark:hover:border-neutral-700'
                                            }`}
                                        style={{ fontFamily: "'Space Mono', monospace" }}
                                    >
                                        {val}
                                    </button>
                                ))}
                            </div>
                        </div>

                        <div>
                            <Label className="text-[10px] uppercase tracking-[0.2em] text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                                Disk (GB)
                            </Label>
                            <Input
                                type="number"
                                value={formData.limits.disk}
                                onChange={(e) => setFormData({
                                    ...formData,
                                    limits: { ...formData.limits, disk: e.target.value }
                                })}
                                min="1"
                                max="100"
                                className="bg-neutral-50 dark:bg-neutral-900/50 border-neutral-300 dark:border-neutral-800/50"
                            />
                            <div className="mt-2 flex gap-2">
                                {['5', '10', '25', '50'].map(val => (
                                    <button
                                        key={val}
                                        type="button"
                                        onClick={() => setFormData({ ...formData, limits: { ...formData.limits, disk: val } })}
                                        className={`px-2 py-1 text-xs border transition-colors ${formData.limits.disk === val
                                            ? 'bg-red-500/20 border-red-500/50 text-red-600 dark:text-red-400'
                                            : 'bg-neutral-50 dark:bg-neutral-900/50 border-neutral-300 dark:border-neutral-800/50 text-neutral-600 dark:text-neutral-400 hover:border-neutral-400 dark:hover:border-neutral-700'
                                            }`}
                                        style={{ fontFamily: "'Space Mono', monospace" }}
                                    >
                                        {val}
                                    </button>
                                ))}
                            </div>
                        </div>

                        <div>
                            <Label className="text-[10px] uppercase tracking-[0.2em] text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                                CPU (%)
                            </Label>
                            <Input
                                type="number"
                                value={formData.limits.cpu}
                                onChange={(e) => setFormData({
                                    ...formData,
                                    limits: { ...formData.limits, cpu: e.target.value }
                                })}
                                min="10"
                                max="400"
                                className="bg-neutral-50 dark:bg-neutral-900/50 border-neutral-300 dark:border-neutral-800/50"
                            />
                            <div className="mt-2 flex gap-2">
                                {['25', '50', '100', '200'].map(val => (
                                    <button
                                        key={val}
                                        type="button"
                                        onClick={() => setFormData({ ...formData, limits: { ...formData.limits, cpu: val } })}
                                        className={`px-2 py-1 text-xs border transition-colors ${formData.limits.cpu === val
                                            ? 'bg-red-500/20 border-red-500/50 text-red-600 dark:text-red-400'
                                            : 'bg-neutral-50 dark:bg-neutral-900/50 border-neutral-300 dark:border-neutral-800/50 text-neutral-600 dark:text-neutral-400 hover:border-neutral-400 dark:hover:border-neutral-700'
                                            }`}
                                        style={{ fontFamily: "'Space Mono', monospace" }}
                                    >
                                        {val}
                                    </button>
                                ))}
                            </div>
                        </div>
                    </div>
                </div>

                {/* Node Selection */}
                <div className="bg-neutral-100 dark:bg-neutral-900/30 border border-neutral-300 dark:border-neutral-800/50 p-6">
                    <div className="flex items-center gap-3 mb-6">
                        <h2
                            className="text-lg text-neutral-900 dark:text-neutral-100 tracking-wider"
                            style={{ fontFamily: "'Seven Segment', sans-serif" }}
                        >
                            NODE
                        </h2>
                        <div className="flex-1 h-px bg-neutral-300 dark:bg-neutral-800"></div>
                    </div>

                    <div className="grid md:grid-cols-2 lg:grid-cols-3 gap-3">
                        {nodes.map((node) => (
                            <button
                                key={node.id}
                                type="button"
                                onClick={() => setFormData({ ...formData, nodeId: node.id })}
                                className={`p-4 text-left border transition-colors ${formData.nodeId === node.id
                                    ? 'bg-red-500/10 border-red-500/50'
                                    : 'bg-neutral-50 dark:bg-neutral-900/50 border-neutral-300 dark:border-neutral-800/50 hover:border-neutral-400 dark:hover:border-neutral-700'
                                    }`}
                            >
                                <div className="text-sm font-medium text-neutral-900 dark:text-neutral-100" style={{ fontFamily: "'Space Mono', monospace" }}>
                                    {node.name}
                                </div>
                                <div className="text-[10px] text-neutral-600 dark:text-neutral-500 mt-1" style={{ fontFamily: "'Space Mono', monospace" }}>
                                    {node.network.scheme}://{node.network.uri}
                                </div>
                            </button>
                        ))}
                    </div>
                </div>

                {/* Submit */}
                <div className="flex justify-end gap-3">
                    <Button
                        type="button"
                        variant="ghost"
                        onClick={() => navigate('/servers')}
                        className="border border-neutral-300 dark:border-neutral-800/50"
                    >
                        Cancel
                    </Button>
                    <Button
                        type="submit"
                        className="bg-red-600 hover:bg-red-700 text-white border-transparent"
                        style={{ fontFamily: "'Space Mono', monospace" }}
                    >
                        [ CREATE SERVER ]
                    </Button>
                </div>
            </form>
        </div>
    )
}
