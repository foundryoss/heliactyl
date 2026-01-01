import React, { useState, useEffect, useCallback } from 'react'
import { useAuth } from '@/providers/AuthProvider'
import { useApi } from '@/api/client'
import { useAlert } from '@/components/ui/Alert'
import Button from '@/components/ui/Button'
import Modal from '@/components/ui/Modal'
import { Input } from '@/components/ui/Input'
import Label from '@/components/ui/Label'
import { CodeBracketIcon, TrashIcon, PencilIcon, PlusIcon, DocumentArrowUpIcon } from '@heroicons/react/24/outline'

interface Software {
	id: string
	name: string
	icon_url?: string
	docker_images: Record<string, string>
	startup_cmd: string
	install_content: string
	update_content: string
	runtime: {
		'file-init': string
		'start-up': string
		stop: string
	}
	variables: Array<{
		name: string
		description: string
		env_variable: string
		default_value: string
		user_viewable: boolean
		user_editable: boolean
		rules: string
		field_type: string
	}>
	created_at: string
}

export function AdminSoftware() {
	const { token } = useAuth()
	const getTokenFn = useCallback(() => token, [token])
	const api = useApi(getTokenFn)
	const { notify } = useAlert()

	const [software, setSoftware] = useState<Software[]>([])
	const [loading, setLoading] = useState(true)
	const [createOpen, setCreateOpen] = useState(false)
	const [editSoftware, setEditSoftware] = useState<Software | null>(null)
	const [deleteSoftware, setDeleteSoftware] = useState<Software | null>(null)

	const fetchSoftware = useCallback(async () => {
		try {
			setLoading(true)
			const res = await api.admin.software()
			setSoftware(res.items || [])
		} catch (e: any) {
			notify({ type: 'error', title: 'Failed to load software', description: e?.message })
		} finally {
			setLoading(false)
		}
	}, [api, notify])

	useEffect(() => {
		fetchSoftware()
	}, [fetchSoftware])

	return (
		<div className="space-y-6">
			<div className="flex items-center justify-between">
				<div>
					<h1 className="text-2xl font-semibold text-neutral-900 dark:text-neutral-100">Server Software</h1>
					<p className="text-sm text-neutral-500 dark:text-neutral-400 mt-1">Manage server software configurations</p>
				</div>
				<Button variant="primary" size="sm" onClick={() => setCreateOpen(true)}>
					<PlusIcon className="h-4 w-4 mr-2" /> Add Software
				</Button>
			</div>

			<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
				{loading ? (
					<p className="text-neutral-500">Loading...</p>
				) : software.length === 0 ? (
					<p className="text-neutral-500">No software configured</p>
				) : (
					software.map((sw) => (
						<div key={sw.id} className="bg-white dark:bg-neutral-800 border border-neutral-200 dark:border-neutral-700 rounded-lg p-4">
							<div className="flex items-start justify-between mb-3">
								<div className="flex items-center gap-2">
									{sw.icon_url ? (
										<img src={sw.icon_url} alt={sw.name} className="h-5 w-5 rounded" />
									) : (
										<CodeBracketIcon className="h-5 w-5 text-neutral-500" />
									)}
									<h3 className="font-medium text-neutral-900 dark:text-neutral-100">{sw.name}</h3>
								</div>
								<div className="flex gap-1">
									<button
										onClick={() => setEditSoftware(sw)}
										className="p-1 rounded hover:bg-neutral-100 dark:hover:bg-neutral-700 transition-colors"
									>
										<PencilIcon className="h-4 w-4 text-neutral-500" />
									</button>
									<button
										onClick={() => setDeleteSoftware(sw)}
										className="p-1 rounded hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors"
									>
										<TrashIcon className="h-4 w-4 text-red-500" />
									</button>
								</div>
							</div>
							<div className="space-y-1 text-xs">
								<div className="flex justify-between">
									<span className="text-neutral-500">Images:</span>
									<span className="text-neutral-900 dark:text-neutral-100">{Object.keys(sw.docker_images).length}</span>
								</div>
								<div className="flex justify-between">
									<span className="text-neutral-500">Variables:</span>
									<span className="text-neutral-900 dark:text-neutral-100">{sw.variables.length}</span>
								</div>
								<div className="flex justify-between">
									<span className="text-neutral-500">Startup:</span>
									<span className="text-neutral-900 dark:text-neutral-100 truncate max-w-[150px]" title={sw.startup_cmd}>{sw.startup_cmd}</span>
								</div>
							</div>
						</div>
					))
				)}
			</div>

			<CreateSoftwareModal open={createOpen} onClose={() => setCreateOpen(false)} onSave={fetchSoftware} />
			<EditSoftwareModal software={editSoftware} onClose={() => setEditSoftware(null)} onSave={fetchSoftware} />
			<DeleteSoftwareModal software={deleteSoftware} onClose={() => setDeleteSoftware(null)} onDelete={fetchSoftware} />
		</div>
	)
}

function CreateSoftwareModal({ open, onClose, onSave }: { open: boolean; onClose: () => void; onSave: () => void }) {
	const { token } = useAuth()
	const getTokenFn = useCallback(() => token, [token])
	const api = useApi(getTokenFn)
	const { notify } = useAlert()
	const [loading, setLoading] = useState(false)
	const [jsonMode, setJsonMode] = useState(false)
	const [jsonInput, setJsonInput] = useState('')

	const handleJsonImport = async (e: React.FormEvent) => {
		e.preventDefault()
		setLoading(true)
		try {
			const data = JSON.parse(jsonInput)
			await api.admin.createSoftware(data)
			notify({ type: 'success', title: 'Software created' })
			onSave()
			onClose()
			setJsonInput('')
		} catch (e: any) {
			notify({ type: 'error', title: 'Failed to create software', description: e?.message || 'Invalid JSON' })
		} finally {
			setLoading(false)
		}
	}

	const handleFileUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
		const file = e.target.files?.[0]
		if (!file) return
		const reader = new FileReader()
		reader.onload = (ev) => {
			setJsonInput(ev.target?.result as string)
		}
		reader.readAsText(file)
	}

	return (
		<Modal open={open} onClose={onClose} title="Add Server Software" subtitle="Import from JSON">
			<form onSubmit={handleJsonImport} className="space-y-4">
				<div>
					<Label htmlFor="json-input">JSON Configuration</Label>
					<textarea
						id="json-input"
						value={jsonInput}
						onChange={(e) => setJsonInput(e.target.value)}
						placeholder='{"name": "Node.js", "docker_images": {...}, ...}'
						className="w-full px-3 py-2 text-sm border border-neutral-200 dark:border-neutral-700 rounded-lg bg-white dark:bg-neutral-800 font-mono min-h-[200px]"
						required
					/>
				</div>
				<div>
					<Label htmlFor="file-upload">Or Upload JSON File</Label>
					<input
						id="file-upload"
						type="file"
						accept=".json"
						onChange={handleFileUpload}
						className="w-full px-3 py-2 text-sm border border-neutral-200 dark:border-neutral-700 rounded-lg bg-white dark:bg-neutral-800"
					/>
				</div>
				<div className="flex justify-end gap-2 pt-2">
					<Button type="button" variant="ghost" size="sm" onClick={onClose}>Cancel</Button>
					<Button type="submit" variant="primary" size="sm" isLoading={loading}>Create Software</Button>
				</div>
			</form>
		</Modal>
	)
}

function EditSoftwareModal({ software, onClose, onSave }: { software: Software | null; onClose: () => void; onSave: () => void }) {
	const { token } = useAuth()
	const getTokenFn = useCallback(() => token, [token])
	const api = useApi(getTokenFn)
	const { notify } = useAlert()
	const [loading, setLoading] = useState(false)
	const [jsonInput, setJsonInput] = useState('')

	useEffect(() => {
		if (software) {
			const { id, created_at, ...rest } = software
			setJsonInput(JSON.stringify(rest, null, 2))
		}
	}, [software])

	const handleSubmit = async (e: React.FormEvent) => {
		e.preventDefault()
		if (!software) return

		setLoading(true)
		try {
			const data = JSON.parse(jsonInput)
			await api.admin.updateSoftware(software.id, data)
			notify({ type: 'success', title: 'Software updated' })
			onSave()
			onClose()
		} catch (e: any) {
			notify({ type: 'error', title: 'Failed to update software', description: e?.message || 'Invalid JSON' })
		} finally {
			setLoading(false)
		}
	}

	return (
		<Modal open={!!software} onClose={onClose} title="Edit Software" subtitle={software?.name}>
			<form onSubmit={handleSubmit} className="space-y-4">
				<div>
					<Label htmlFor="edit-json">JSON Configuration</Label>
					<textarea
						id="edit-json"
						value={jsonInput}
						onChange={(e) => setJsonInput(e.target.value)}
						className="w-full px-3 py-2 text-sm border border-neutral-200 dark:border-neutral-700 rounded-lg bg-white dark:bg-neutral-800 font-mono min-h-[300px]"
						required
					/>
				</div>
				<div className="flex justify-end gap-2 pt-2">
					<Button type="button" variant="ghost" size="sm" onClick={onClose}>Cancel</Button>
					<Button type="submit" variant="primary" size="sm" isLoading={loading}>Save Changes</Button>
				</div>
			</form>
		</Modal>
	)
}

function DeleteSoftwareModal({ software, onClose, onDelete }: { software: Software | null; onClose: () => void; onDelete: () => void }) {
	const { token } = useAuth()
	const getTokenFn = useCallback(() => token, [token])
	const api = useApi(getTokenFn)
	const { notify } = useAlert()
	const [loading, setLoading] = useState(false)

	const handleDelete = async () => {
		if (!software) return
		setLoading(true)
		try {
			await api.admin.deleteSoftware(software.id)
			notify({ type: 'success', title: 'Software deleted' })
			onDelete()
			onClose()
		} catch (e: any) {
			notify({ type: 'error', title: 'Failed to delete', description: e?.message })
		} finally {
			setLoading(false)
		}
	}

	return (
		<Modal open={!!software} onClose={onClose} title="Delete Software" subtitle="This action cannot be undone">
			<div className="space-y-4">
				<p className="text-sm text-neutral-600 dark:text-neutral-400">
					Are you sure you want to delete <span className="font-semibold">{software?.name}</span>?
				</p>
				<div className="flex justify-end gap-2">
					<Button type="button" variant="ghost" size="sm" onClick={onClose}>Cancel</Button>
					<Button type="button" variant="destructive" size="sm" isLoading={loading} onClick={handleDelete}>Delete</Button>
				</div>
			</div>
		</Modal>
	)
}
