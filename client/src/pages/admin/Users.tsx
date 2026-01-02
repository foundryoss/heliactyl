import React, { useState, useEffect, useCallback } from 'react'
import { useAuth } from '@/providers/AuthProvider'
import { useApi } from '@/api/client'
import { useAlert } from '@/components/ui/Alert'
import Button from '@/components/ui/Button'
import Modal from '@/components/ui/Modal'
import { Input } from '@/components/ui/Input'
import Label from '@/components/ui/Label'
import { cn } from '@/utils/cn'
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from '@/components/ui/DropdownMenu'
import {
	ShieldCheckIcon,
	TrashIcon,
	PencilIcon,
	MagnifyingGlassIcon,
	KeyIcon,
	EllipsisVerticalIcon,
	CurrencyDollarIcon,
} from '@heroicons/react/24/outline'

interface User {
	id: string
	email: string
	username: string
	is_admin: boolean
	created_at: string
}

export function AdminUsers() {
	const { token, user: currentUser } = useAuth()
	const getTokenFn = useCallback(() => token, [token])
	const api = useApi(getTokenFn)
	const { notify } = useAlert()

	const [users, setUsers] = useState<User[]>([])
	const [loading, setLoading] = useState(true)
	const [search, setSearch] = useState('')
	const [editUser, setEditUser] = useState<User | null>(null)
	const [deleteUser, setDeleteUser] = useState<User | null>(null)
	const [resetPasswordUser, setResetPasswordUser] = useState<User | null>(null)
	const [resourceUnitsUser, setResourceUnitsUser] = useState<User | null>(null)

	const fetchUsers = useCallback(async () => {
		try {
			setLoading(true)
			const res = await api.admin.users()
			setUsers(res.items || [])
		} catch (e: any) {
			notify({ type: 'error', title: 'Failed to load users', description: e?.message })
		} finally {
			setLoading(false)
		}
	}, [api, notify])

	useEffect(() => {
		fetchUsers()
	}, [fetchUsers])

	const handleToggleAdmin = async (user: User) => {
		try {
			await api.admin.setAdmin(user.id, !user.is_admin)
			notify({ type: 'success', title: 'Updated', description: `${user.username} is ${!user.is_admin ? 'now' : 'no longer'} an admin` })
			fetchUsers()
		} catch (e: any) {
			notify({ type: 'error', title: 'Failed to update', description: e?.message })
		}
	}

	const filteredUsers = users.filter(
		(u) =>
			u.email.toLowerCase().includes(search.toLowerCase()) ||
			u.username.toLowerCase().includes(search.toLowerCase())
	)

	return (
		<div className="space-y-6">
			<div className="flex items-center justify-between">
				<div>
					<h1 className="text-2xl font-semibold text-neutral-900 dark:text-neutral-100">Users</h1>
					<p className="text-sm text-neutral-500 dark:text-neutral-400 mt-1">Manage user accounts and permissions</p>
				</div>
				<div className="text-sm text-neutral-500">{users.length} users</div>
			</div>

			{/* Search */}
			<div className="relative max-w-md">
				<MagnifyingGlassIcon className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-neutral-400" />
				<input
					type="text"
					placeholder="Search by email or username..."
					value={search}
					onChange={(e) => setSearch(e.target.value)}
					className="w-full pl-10 pr-4 py-2 text-sm border border-neutral-200 dark:border-neutral-700 rounded-lg bg-white dark:bg-neutral-800 text-neutral-900 dark:text-neutral-100 placeholder-neutral-400 focus:outline-none focus:ring-2 focus:ring-neutral-900 dark:focus:ring-neutral-100"
				/>
			</div>

			{/* Users Table */}
			<div className="bg-white dark:bg-neutral-800 border border-neutral-200 dark:border-neutral-700 rounded-lg overflow-hidden">
				<table className="w-full text-sm w-full">
					<thead className="bg-neutral-50 dark:bg-neutral-900">
						<tr>
							<th className="text-left px-4 py-3 font-medium text-neutral-600 dark:text-neutral-300">User</th>
							<th className="text-left px-4 py-3 font-medium text-neutral-600 dark:text-neutral-300">Email</th>
							<th className="text-left px-4 py-3 font-medium text-neutral-600 dark:text-neutral-300">Role</th>
							<th className="text-left px-4 py-3 font-medium text-neutral-600 dark:text-neutral-300">Created</th>
							<th className="text-right px-4 py-3 font-medium text-neutral-600 dark:text-neutral-300">Actions</th>
						</tr>
					</thead>
					<tbody className="divide-y divide-neutral-200 dark:divide-neutral-700 mb-30">
						{loading ? (
							<tr>
								<td colSpan={5} className="px-4 py-8 text-center text-neutral-500">Loading...</td>
							</tr>
						) : filteredUsers.length === 0 ? (
							<tr>
								<td colSpan={5} className="px-4 py-8 text-center text-neutral-500">No users found</td>
							</tr>
						) : (
							filteredUsers.map((user) => (
								<tr key={user.id} className="hover:bg-neutral-50 dark:hover:bg-neutral-800/50">
									<td className="px-4 py-3">
										<div className="flex items-center gap-3">
											<div className="w-8 h-8 rounded-full bg-neutral-200 dark:bg-neutral-700 flex items-center justify-center">
												<span className="text-xs font-medium text-neutral-600 dark:text-neutral-300 uppercase">
													{user.username?.charAt(0) || 'U'}
												</span>
											</div>
											<div>
												<span className="font-medium text-neutral-900 dark:text-neutral-100">{user.username}</span>
												{user.id === currentUser?.id && (
													<span className="ml-2 text-xs text-neutral-400">(you)</span>
												)}
											</div>
										</div>
									</td>
									<td className="px-4 py-3 text-neutral-600 dark:text-neutral-400">{user.email}</td>
									<td className="px-4 py-3">
										{user.is_admin ? (
											<span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium bg-yellow-100 text-yellow-800 dark:bg-yellow-900/30 dark:text-yellow-400">
												<ShieldCheckIcon className="h-3 w-3" /> Admin
											</span>
										) : (
											<span className="text-neutral-500 dark:text-neutral-400 text-xs">User</span>
										)}
									</td>
									<td className="px-4 py-3 text-neutral-500 dark:text-neutral-400 text-xs">
										{new Date(user.created_at).toLocaleDateString()}
									</td>
									<td className="px-4 py-3">
										<div className="flex items-center justify-end">
											<DropdownMenu>
												<DropdownMenuTrigger asChild>
													<button className="p-1.5 rounded hover:bg-neutral-100 dark:hover:bg-neutral-700 transition-colors">
														<EllipsisVerticalIcon className="h-5 w-5 text-neutral-500" />
													</button>
												</DropdownMenuTrigger>
												<DropdownMenuContent align="end" className="w-48">
													<DropdownMenuItem onClick={() => setEditUser(user)}>
														<PencilIcon className="h-4 w-4 mr-2" />
														Edit Details
													</DropdownMenuItem>
													<DropdownMenuItem onClick={() => setResetPasswordUser(user)}>
														<KeyIcon className="h-4 w-4 mr-2" />
														Reset Password
													</DropdownMenuItem>
													<DropdownMenuItem onClick={() => setResourceUnitsUser(user)}>
														<CurrencyDollarIcon className="h-4 w-4 mr-2" />
														Manage Resource Units
													</DropdownMenuItem>
													<DropdownMenuItem
														onClick={() => handleToggleAdmin(user)}
														disabled={user.id === currentUser?.id}
													>
														<ShieldCheckIcon className="h-4 w-4 mr-2" />
														{user.is_admin ? 'Remove Admin' : 'Make Admin'}
													</DropdownMenuItem>
													<DropdownMenuSeparator />
													<DropdownMenuItem
														onClick={() => setDeleteUser(user)}
														disabled={user.id === currentUser?.id}
														className="text-red-600 dark:text-red-400 focus:text-red-600 dark:focus:text-red-400"
													>
														<TrashIcon className="h-4 w-4 mr-2" />
														Delete User
													</DropdownMenuItem>
												</DropdownMenuContent>
											</DropdownMenu>
										</div>
									</td>
								</tr>
							))
						)}
					</tbody>
				</table>
			</div>

			{/* Modals */}
			<EditUserModal user={editUser} onClose={() => setEditUser(null)} onSave={fetchUsers} />
			<DeleteUserModal user={deleteUser} onClose={() => setDeleteUser(null)} onDelete={fetchUsers} />
			<ResetPasswordModal user={resetPasswordUser} onClose={() => setResetPasswordUser(null)} />
			<ManageResourceUnitsModal user={resourceUnitsUser} onClose={() => setResourceUnitsUser(null)} />
		</div>
	)
}

function EditUserModal({ user, onClose, onSave }: { user: User | null; onClose: () => void; onSave: () => void }) {
	const { token } = useAuth()
	const { notify } = useAlert()

	const [email, setEmail] = useState('')
	const [username, setUsername] = useState('')
	const [loading, setLoading] = useState(false)

	useEffect(() => {
		if (user) {
			setEmail(user.email)
			setUsername(user.username)
		}
	}, [user])

	const handleSubmit = async (e: React.FormEvent) => {
		e.preventDefault()
		if (!user) return

		setLoading(true)
		try {
			const res = await fetch(`/api/admin/users/${user.id}`, {
				method: 'PATCH',
				headers: {
					'Content-Type': 'application/json',
					Authorization: `Bearer ${token}`,
				},
				body: JSON.stringify({ email, username }),
			})
			if (!res.ok) throw new Error('Failed to update')
			notify({ type: 'success', title: 'User updated' })
			onSave()
			onClose()
		} catch (e: any) {
			notify({ type: 'error', title: 'Failed to update', description: e?.message })
		} finally {
			setLoading(false)
		}
	}

	return (
		<Modal open={!!user} onClose={onClose} title="Edit User" subtitle={user?.email}>
			<form onSubmit={handleSubmit} className="space-y-4">
				<div>
					<Label htmlFor="edit-email">Email</Label>
					<Input id="edit-email" type="email" value={email} onChange={(e) => setEmail(e.target.value)} />
				</div>
				<div>
					<Label htmlFor="edit-username">Username</Label>
					<Input id="edit-username" value={username} onChange={(e) => setUsername(e.target.value)} />
				</div>
				<div className="flex justify-end gap-2 pt-2">
					<Button type="button" variant="ghost" size="sm" onClick={onClose}>Cancel</Button>
					<Button type="submit" variant="primary" size="sm" isLoading={loading}>Save Changes</Button>
				</div>
			</form>
		</Modal>
	)
}

function DeleteUserModal({ user, onClose, onDelete }: { user: User | null; onClose: () => void; onDelete: () => void }) {
	const { token } = useAuth()
	const { notify } = useAlert()
	const [loading, setLoading] = useState(false)

	const handleDelete = async () => {
		if (!user) return

		setLoading(true)
		try {
			const res = await fetch(`/api/admin/users/${user.id}`, {
				method: 'DELETE',
				headers: { Authorization: `Bearer ${token}` },
			})
			if (!res.ok) throw new Error('Failed to delete')
			notify({ type: 'success', title: 'User deleted' })
			onDelete()
			onClose()
		} catch (e: any) {
			notify({ type: 'error', title: 'Failed to delete', description: e?.message })
		} finally {
			setLoading(false)
		}
	}

	return (
		<Modal open={!!user} onClose={onClose} title="Delete User" subtitle="This action cannot be undone">
			<div className="space-y-4">
				<p className="text-sm text-neutral-600 dark:text-neutral-400">
					Are you sure you want to delete <span className="font-semibold">{user?.username}</span>? This will permanently remove their account and all associated data.
				</p>
				<div className="flex justify-end gap-2">
					<Button type="button" variant="ghost" size="sm" onClick={onClose}>Cancel</Button>
					<Button type="button" variant="destructive" size="sm" isLoading={loading} onClick={handleDelete}>Delete User</Button>
				</div>
			</div>
		</Modal>
	)
}

function ResetPasswordModal({ user, onClose }: { user: User | null; onClose: () => void }) {
	const { token } = useAuth()
	const { notify } = useAlert()

	const [password, setPassword] = useState('')
	const [confirmPassword, setConfirmPassword] = useState('')
	const [loading, setLoading] = useState(false)

	useEffect(() => {
		if (!user) {
			setPassword('')
			setConfirmPassword('')
		}
	}, [user])

	const handleSubmit = async (e: React.FormEvent) => {
		e.preventDefault()
		if (!user) return

		if (password !== confirmPassword) {
			notify({ type: 'error', title: 'Passwords do not match' })
			return
		}

		if (password.length < 6) {
			notify({ type: 'error', title: 'Password must be at least 6 characters' })
			return
		}

		setLoading(true)
		try {
			const res = await fetch(`/api/admin/users/${user.id}/reset-password`, {
				method: 'POST',
				headers: {
					'Content-Type': 'application/json',
					Authorization: `Bearer ${token}`,
				},
				body: JSON.stringify({ password }),
			})
			if (!res.ok) throw new Error('Failed to reset password')
			notify({ type: 'success', title: 'Password reset', description: `Password for ${user.username} has been reset` })
			onClose()
		} catch (e: any) {
			notify({ type: 'error', title: 'Failed to reset password', description: e?.message })
		} finally {
			setLoading(false)
		}
	}

	return (
		<Modal open={!!user} onClose={onClose} title="Reset Password" subtitle={user?.username}>
			<form onSubmit={handleSubmit} className="space-y-4">
				<div>
					<Label htmlFor="new-password">New Password</Label>
					<Input
						id="new-password"
						type="password"
						value={password}
						onChange={(e) => setPassword(e.target.value)}
						placeholder="Enter new password"
					/>
				</div>
				<div>
					<Label htmlFor="confirm-password">Confirm Password</Label>
					<Input
						id="confirm-password"
						type="password"
						value={confirmPassword}
						onChange={(e) => setConfirmPassword(e.target.value)}
						placeholder="Confirm new password"
					/>
				</div>
				<div className="flex justify-end gap-2 pt-2">
					<Button type="button" variant="ghost" size="sm" onClick={onClose}>Cancel</Button>
					<Button type="submit" variant="primary" size="sm" isLoading={loading}>Reset Password</Button>
				</div>
			</form>
		</Modal>
	)
}


function ManageResourceUnitsModal({ user, onClose }: { user: User | null; onClose: () => void }) {
	const { token } = useAuth()
	const getTokenFn = useCallback(() => token, [token])
	const api = useApi(getTokenFn)
	const { notify } = useAlert()

	const [balance, setBalance] = useState<number>(0)
	const [currentBalance, setCurrentBalance] = useState<number>(0)
	const [reason, setReason] = useState('')
	const [loading, setLoading] = useState(false)
	const [fetching, setFetching] = useState(true)

	useEffect(() => {
		if (user) {
			setFetching(true)
			api.admin.getUserWallet(user.id)
				.then((res) => {
					setCurrentBalance(res.ruBalance)
					setBalance(res.ruBalance)
				})
				.catch((e) => {
					notify({ type: 'error', title: 'Failed to load wallet', description: e?.message })
				})
				.finally(() => setFetching(false))
		} else {
			setBalance(0)
			setCurrentBalance(0)
			setReason('')
		}
	}, [user, api, notify])

	const handleSubmit = async (e: React.FormEvent) => {
		e.preventDefault()
		if (!user) return

		setLoading(true)
		try {
			const res = await api.admin.setResourceUnits(user.id, balance, reason || undefined)
			notify({ 
				type: 'success', 
				title: 'Resource Units updated', 
				description: `Balance changed from ${res.oldBalance.toFixed(2)} to ${res.newBalance.toFixed(2)} RU` 
			})
			setCurrentBalance(res.newBalance)
			onClose()
		} catch (e: any) {
			notify({ type: 'error', title: 'Failed to update resource units', description: e?.message })
		} finally {
			setLoading(false)
		}
	}

	const handleQuickAdd = (amount: number) => {
		setBalance(prev => Math.max(0, prev + amount))
	}

	return (
		<Modal open={!!user} onClose={onClose} title="Manage Resource Units" subtitle={user?.username}>
			{fetching ? (
				<div className="py-8 text-center text-neutral-500">Loading wallet...</div>
			) : (
				<form onSubmit={handleSubmit} className="space-y-4">
					<div className="bg-neutral-100 dark:bg-neutral-900 rounded-lg p-4">
						<div className="text-sm text-neutral-500 dark:text-neutral-400">Current Balance</div>
						<div className="text-2xl font-semibold text-neutral-900 dark:text-neutral-100">
							{currentBalance.toFixed(2)} <span className="text-sm font-normal text-neutral-500">RU</span>
						</div>
					</div>

					<div>
						<Label htmlFor="new-balance">New Balance (RU)</Label>
						<Input
							id="new-balance"
							type="number"
							step="0.01"
							min="0"
							value={balance}
							onChange={(e) => setBalance(parseFloat(e.target.value) || 0)}
						/>
					</div>

					<div className="flex gap-2 flex-wrap">
						<button
							type="button"
							onClick={() => handleQuickAdd(10)}
							className="px-3 py-1 text-xs rounded bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400 hover:bg-green-200 dark:hover:bg-green-900/50"
						>
							+10 RU
						</button>
						<button
							type="button"
							onClick={() => handleQuickAdd(50)}
							className="px-3 py-1 text-xs rounded bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400 hover:bg-green-200 dark:hover:bg-green-900/50"
						>
							+50 RU
						</button>
						<button
							type="button"
							onClick={() => handleQuickAdd(100)}
							className="px-3 py-1 text-xs rounded bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400 hover:bg-green-200 dark:hover:bg-green-900/50"
						>
							+100 RU
						</button>
						<button
							type="button"
							onClick={() => handleQuickAdd(-10)}
							className="px-3 py-1 text-xs rounded bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400 hover:bg-red-200 dark:hover:bg-red-900/50"
						>
							-10 RU
						</button>
						<button
							type="button"
							onClick={() => setBalance(0)}
							className="px-3 py-1 text-xs rounded bg-neutral-200 text-neutral-700 dark:bg-neutral-700 dark:text-neutral-300 hover:bg-neutral-300 dark:hover:bg-neutral-600"
						>
							Reset to 0
						</button>
					</div>

					{balance !== currentBalance && (
						<div className={cn(
							'text-sm px-3 py-2 rounded',
							balance > currentBalance 
								? 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400'
								: 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400'
						)}>
							{balance > currentBalance ? '+' : ''}{(balance - currentBalance).toFixed(2)} RU change
						</div>
					)}

					<div>
						<Label htmlFor="ru-reason">Reason (optional)</Label>
						<Input
							id="ru-reason"
							value={reason}
							onChange={(e) => setReason(e.target.value)}
							placeholder="e.g., Promotional credit, refund, etc."
						/>
					</div>

					<div className="flex justify-end gap-2 pt-2">
						<Button type="button" variant="ghost" size="sm" onClick={onClose}>Cancel</Button>
						<Button 
							type="submit" 
							variant="primary" 
							size="sm" 
							isLoading={loading}
							disabled={balance === currentBalance}
						>
							Update Resource Units
						</Button>
					</div>
				</form>
			)}
		</Modal>
	)
}
