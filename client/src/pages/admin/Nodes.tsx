import React, { useState, useEffect, useCallback } from 'react'
import { useAuth } from '@/providers/AuthProvider'
import { useApi } from '@/api/client'
import { useAlert } from '@/components/ui/Alert'
import Button from '@/components/ui/Button'
import Modal from '@/components/ui/Modal'
import { Input } from '@/components/ui/Input'
import Label from '@/components/ui/Label'
import { ServerStackIcon, TrashIcon, PencilIcon, PlusIcon } from '@heroicons/react/24/outline'

interface Node {
	id: string
	name: string
	icon_url?: string
	limits: { cpu: string; memory: string; disk: string }
	network: { uri: string; scheme: string; authType: string; ip: string; fqdn: string }
	created_at: string
}

export function AdminNodes() {
	const { token } = useAuth()
	const getTokenFn = useCallback(() => token, [token])
	const api = useApi(getTokenFn)
	const { notify } = useAlert()

	const [nodes, setNodes] = useState<Node[]>([])
	const [loading, setLoading] = useState(true)
	const [createOpen, setCreateOpen] = useState(false)
	const [editNode, setEditNode] = useState<Node | null>(null)
	const [deleteNode, setDeleteNode] = useState<Node | null>(null)

	const fetchNodes = useCallback(async () => {
		try {
			setLoading(true)
			const res = await api.admin.nodes()
			setNodes(res.items || [])
		} catch (e: any) {
			notify({ type: 'error', title: 'Failed to load nodes', description: e?.message })
		} finally {
			setLoading(false)
		}
	}, [api, notify])

	useEffect(() => {
		fetchNodes()
	}, [fetchNodes])

	return (
		<div className="space-y-6">
			<div className="flex items-center justify-between">
				<div>
					<h1 className="text-2xl font-semibold text-neutral-900 dark:text-neutral-100">Nodes</h1>
					<p className="text-sm text-neutral-500 dark:text-neutral-400 mt-1">Manage daemon nodes</p>
				</div>
				<Button variant="primary" size="sm" onClick={() => setCreateOpen(true)}>
					<PlusIcon className="h-4 w-4 mr-2" /> Add Node
				</Button>
			</div>

			<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
				{loading ? (
					<p className="text-neutral-500">Loading...</p>
				) : nodes.length === 0 ? (
					<p className="text-neutral-500">No nodes configured</p>
				) : (
					nodes.map((node) => (
						<div key={node.id} className="bg-white dark:bg-neutral-800 border border-neutral-200 dark:border-neutral-700 rounded-lg p-4">
							<div className="flex items-start justify-between mb-3">
								<div className="flex items-center gap-2">
									{node.icon_url ? (
										<img src={node.icon_url} alt={node.name} className="h-5 w-5 rounded" />
									) : (
										<ServerStackIcon className="h-5 w-5 text-neutral-500" />
									)}
									<h3 className="font-medium text-neutral-900 dark:text-neutral-100">{node.name}</h3>
								</div>
								<div className="flex gap-1">
									<button
										onClick={() => setEditNode(node)}
										className="p-1 rounded hover:bg-neutral-100 dark:hover:bg-neutral-700 transition-colors"
									>
										<PencilIcon className="h-4 w-4 text-neutral-500" />
									</button>
									<button
										onClick={() => setDeleteNode(node)}
										className="p-1 rounded hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors"
									>
										<TrashIcon className="h-4 w-4 text-red-500" />
									</button>
								</div>
							</div>
							<div className="space-y-1 text-xs">
								<div className="flex justify-between">
									<span className="text-neutral-500">URI:</span>
									<span className="text-neutral-900 dark:text-neutral-100">{node.network.uri}</span>
								</div>
								<div className="flex justify-between">
									<span className="text-neutral-500">CPU:</span>
									<span className="text-neutral-900 dark:text-neutral-100">{node.limits.cpu}</span>
								</div>
								<div className="flex justify-between">
									<span className="text-neutral-500">Memory:</span>
									<span className="text-neutral-900 dark:text-neutral-100">{node.limits.memory}</span>
								</div>
								<div className="flex justify-between">
									<span className="text-neutral-500">Disk:</span>
									<span className="text-neutral-900 dark:text-neutral-100">{node.limits.disk}</span>
								</div>
							</div>
						</div>
					))
				)}
			</div>

			<CreateNodeModal open={createOpen} onClose={() => setCreateOpen(false)} onSave={fetchNodes} />
			<EditNodeModal node={editNode} onClose={() => setEditNode(null)} onSave={fetchNodes} />
			<DeleteNodeModal node={deleteNode} onClose={() => setDeleteNode(null)} onDelete={fetchNodes} />
		</div>
	)
}

function CreateNodeModal({ open, onClose, onSave }: { open: boolean; onClose: () => void; onSave: () => void }) {
	const { token } = useAuth()
	const getTokenFn = useCallback(() => token, [token])
	const api = useApi(getTokenFn)
	const { notify } = useAlert()
	const [loading, setLoading] = useState(false)

	const [formData, setFormData] = useState({
		name: '',
		icon_url: '',
		cpu: '1',
		memory: '1024MB',
		disk: '100GB',
		uri: 'localhost:8070',
		scheme: 'http',
		ip: '0.0.0.0',
	})

	const handleSubmit = async (e: React.FormEvent) => {
		e.preventDefault()
		setLoading(true)
		try {
			await api.admin.createNode({
				name: formData.name,
				icon_url: formData.icon_url || undefined,
				limits: { cpu: formData.cpu, memory: formData.memory, disk: formData.disk },
				network: { uri: formData.uri, scheme: formData.scheme, authType: 'BASIC', ip: formData.ip, fqdn: '' },
				node_config: {
					_note: "Don't mess around with this if you don't know what your doing mate ;)",
					version: '0.1.0',
					server: { host: '0.0.0.0', port: 8070 },
					docker: { socket_path: '/var/run/docker.sock' },
					storage: { base_path: './storage', containers_path: './storage/containers', volumes_path: './storage/volumes' },
					monitoring: {
						enabled: true,
						interval_ms: 1000,
						ru_config: { cpu_weight: 1.0, memory_weight: 0.5, io_weight: 2.0, network_weight: 1.5, storage_weight: 0.8, base_ru: 0.1 },
					},
				},
			})
			notify({ type: 'success', title: 'Node created' })
			onSave()
			onClose()
		} catch (e: any) {
			notify({ type: 'error', title: 'Failed to create node', description: e?.message })
		} finally {
			setLoading(false)
		}
	}

	return (
		<Modal open={open} onClose={onClose} title="Add Node" subtitle="Configure a new daemon node">
			<form onSubmit={handleSubmit} className="space-y-4">
				<div>
					<Label htmlFor="name">Node Name</Label>
					<Input id="name" value={formData.name} onChange={(e) => setFormData({ ...formData, name: e.target.value })} required />
				</div>
				<div>
					<Label htmlFor="icon_url">Icon URL (optional)</Label>
					<Input id="icon_url" type="url" placeholder="https://example.com/icon.png" value={formData.icon_url} onChange={(e) => setFormData({ ...formData, icon_url: e.target.value })} />
				</div>
				<div className="grid grid-cols-3 gap-3">
					<div>
						<Label htmlFor="cpu">CPU</Label>
						<Input id="cpu" value={formData.cpu} onChange={(e) => setFormData({ ...formData, cpu: e.target.value })} />
					</div>
					<div>
						<Label htmlFor="memory">Memory</Label>
						<Input id="memory" value={formData.memory} onChange={(e) => setFormData({ ...formData, memory: e.target.value })} />
					</div>
					<div>
						<Label htmlFor="disk">Disk</Label>
						<Input id="disk" value={formData.disk} onChange={(e) => setFormData({ ...formData, disk: e.target.value })} />
					</div>
				</div>
				<div>
					<Label htmlFor="uri">URI</Label>
					<Input id="uri" value={formData.uri} onChange={(e) => setFormData({ ...formData, uri: e.target.value })} required />
				</div>
				<div className="grid grid-cols-2 gap-3">
					<div>
						<Label htmlFor="scheme">Scheme</Label>
						<select
							id="scheme"
							value={formData.scheme}
							onChange={(e) => setFormData({ ...formData, scheme: e.target.value })}
							className="w-full px-3 py-2 text-sm border border-neutral-200 dark:border-neutral-700 rounded-lg bg-white dark:bg-neutral-800"
						>
							<option value="http">HTTP</option>
							<option value="https">HTTPS</option>
						</select>
					</div>
					<div>
						<Label htmlFor="ip">IP Address</Label>
						<Input id="ip" value={formData.ip} onChange={(e) => setFormData({ ...formData, ip: e.target.value })} />
					</div>
				</div>
				<div className="flex justify-end gap-2 pt-2">
					<Button type="button" variant="ghost" size="sm" onClick={onClose}>Cancel</Button>
					<Button type="submit" variant="primary" size="sm" isLoading={loading}>Create Node</Button>
				</div>
			</form>
		</Modal>
	)
}

function EditNodeModal({ node, onClose, onSave }: { node: Node | null; onClose: () => void; onSave: () => void }) {
	const { token } = useAuth()
	const getTokenFn = useCallback(() => token, [token])
	const api = useApi(getTokenFn)
	const { notify } = useAlert()
	const [loading, setLoading] = useState(false)

	const [formData, setFormData] = useState({
		name: '',
		icon_url: '',
		cpu: '',
		memory: '',
		disk: '',
		uri: '',
		scheme: 'http',
		ip: '',
	})

	useEffect(() => {
		if (node) {
			setFormData({
				name: node.name,
				icon_url: node.icon_url || '',
				cpu: node.limits.cpu,
				memory: node.limits.memory,
				disk: node.limits.disk,
				uri: node.network.uri,
				scheme: node.network.scheme,
				ip: node.network.ip,
			})
		}
	}, [node])

	const handleSubmit = async (e: React.FormEvent) => {
		e.preventDefault()
		if (!node) return

		setLoading(true)
		try {
			await api.admin.updateNode(node.id, {
				name: formData.name,
				icon_url: formData.icon_url || undefined,
				limits: { cpu: formData.cpu, memory: formData.memory, disk: formData.disk },
				network: { uri: formData.uri, scheme: formData.scheme, authType: 'BASIC', ip: formData.ip, fqdn: '' },
			})
			notify({ type: 'success', title: 'Node updated' })
			onSave()
			onClose()
		} catch (e: any) {
			notify({ type: 'error', title: 'Failed to update node', description: e?.message })
		} finally {
			setLoading(false)
		}
	}

	return (
		<Modal open={!!node} onClose={onClose} title="Edit Node" subtitle={node?.name}>
			<form onSubmit={handleSubmit} className="space-y-4">
				<div>
					<Label htmlFor="edit-name">Node Name</Label>
					<Input id="edit-name" value={formData.name} onChange={(e) => setFormData({ ...formData, name: e.target.value })} required />
				</div>
				<div>
					<Label htmlFor="edit-icon_url">Icon URL (optional)</Label>
					<Input id="edit-icon_url" type="url" placeholder="https://example.com/icon.png" value={formData.icon_url} onChange={(e) => setFormData({ ...formData, icon_url: e.target.value })} />
				</div>
				<div className="grid grid-cols-3 gap-3">
					<div>
						<Label htmlFor="edit-cpu">CPU</Label>
						<Input id="edit-cpu" value={formData.cpu} onChange={(e) => setFormData({ ...formData, cpu: e.target.value })} />
					</div>
					<div>
						<Label htmlFor="edit-memory">Memory</Label>
						<Input id="edit-memory" value={formData.memory} onChange={(e) => setFormData({ ...formData, memory: e.target.value })} />
					</div>
					<div>
						<Label htmlFor="edit-disk">Disk</Label>
						<Input id="edit-disk" value={formData.disk} onChange={(e) => setFormData({ ...formData, disk: e.target.value })} />
					</div>
				</div>
				<div>
					<Label htmlFor="edit-uri">URI</Label>
					<Input id="edit-uri" value={formData.uri} onChange={(e) => setFormData({ ...formData, uri: e.target.value })} required />
				</div>
				<div className="grid grid-cols-2 gap-3">
					<div>
						<Label htmlFor="edit-scheme">Scheme</Label>
						<select
							id="edit-scheme"
							value={formData.scheme}
							onChange={(e) => setFormData({ ...formData, scheme: e.target.value })}
							className="w-full px-3 py-2 text-sm border border-neutral-200 dark:border-neutral-700 rounded-lg bg-white dark:bg-neutral-800"
						>
							<option value="http">HTTP</option>
							<option value="https">HTTPS</option>
						</select>
					</div>
					<div>
						<Label htmlFor="edit-ip">IP Address</Label>
						<Input id="edit-ip" value={formData.ip} onChange={(e) => setFormData({ ...formData, ip: e.target.value })} />
					</div>
				</div>
				<div className="flex justify-end gap-2 pt-2">
					<Button type="button" variant="ghost" size="sm" onClick={onClose}>Cancel</Button>
					<Button type="submit" variant="primary" size="sm" isLoading={loading}>Save Changes</Button>
				</div>
			</form>
		</Modal>
	)
}

function DeleteNodeModal({ node, onClose, onDelete }: { node: Node | null; onClose: () => void; onDelete: () => void }) {
	const { token } = useAuth()
	const getTokenFn = useCallback(() => token, [token])
	const api = useApi(getTokenFn)
	const { notify } = useAlert()
	const [loading, setLoading] = useState(false)

	const handleDelete = async () => {
		if (!node) return
		setLoading(true)
		try {
			await api.admin.deleteNode(node.id)
			notify({ type: 'success', title: 'Node deleted' })
			onDelete()
			onClose()
		} catch (e: any) {
			notify({ type: 'error', title: 'Failed to delete', description: e?.message })
		} finally {
			setLoading(false)
		}
	}

	return (
		<Modal open={!!node} onClose={onClose} title="Delete Node" subtitle="This action cannot be undone">
			<div className="space-y-4">
				<p className="text-sm text-neutral-600 dark:text-neutral-400">
					Are you sure you want to delete <span className="font-semibold">{node?.name}</span>?
				</p>
				<div className="flex justify-end gap-2">
					<Button type="button" variant="ghost" size="sm" onClick={onClose}>Cancel</Button>
					<Button type="button" variant="destructive" size="sm" isLoading={loading} onClick={handleDelete}>Delete</Button>
				</div>
			</div>
		</Modal>
	)
}
