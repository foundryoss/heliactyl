import React, { useState, useEffect, useCallback } from 'react'
import { Link } from 'react-router-dom'
import { useAuth } from '@/providers/AuthProvider'
import { useApi } from '@/api/client'
import { useAlert } from '@/components/ui/Alert'
import { cn } from '@/utils/cn'
import {
	UsersIcon,
	ServerStackIcon,
	CpuChipIcon,
	ArrowRightIcon,
} from '@heroicons/react/24/outline'

export function AdminOverview() {
	const { token } = useAuth()
	const getTokenFn = useCallback(() => token, [token])
	const api = useApi(getTokenFn)
	const { notify } = useAlert()

	const [stats, setStats] = useState({ users: 0, admins: 0 })
	const [diagnostics, setDiagnostics] = useState<{ db: { ok: boolean }; redis: { ok: boolean }; daemon: { url: string } } | null>(null)
	const [loading, setLoading] = useState(true)

	useEffect(() => {
		const fetchData = async () => {
			try {
				const [usersRes, diagRes] = await Promise.all([
					api.admin.users(),
					api.admin.diagnostics(),
				])
				const users = usersRes.items || []
				setStats({
					users: users.length,
					admins: users.filter((u: any) => u.is_admin).length,
				})
				setDiagnostics(diagRes)
			} catch (e: any) {
				notify({ type: 'error', title: 'Failed to load data', description: e?.message })
			} finally {
				setLoading(false)
			}
		}
		fetchData()
	}, [api, notify])

	return (
		<div className="space-y-6">
			<div>
				<h1 className="text-2xl font-semibold text-neutral-900 dark:text-neutral-100">Admin Overview</h1>
				<p className="text-sm text-neutral-500 dark:text-neutral-400 mt-1">System status and quick actions</p>
			</div>

			{/* Stats Cards */}
			<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
				<StatCard
					title="Total Users"
					value={loading ? '...' : stats.users.toString()}
					icon={UsersIcon}
					href="/admin/users"
				/>
				<StatCard
					title="Admins"
					value={loading ? '...' : stats.admins.toString()}
					icon={UsersIcon}
					href="/admin/users"
				/>
			</div>

			{/* System Health */}
			<div className="bg-white dark:bg-neutral-800 rounded-lg border border-neutral-200 dark:border-neutral-700 p-6">
				<h2 className="text-lg font-medium text-neutral-900 dark:text-neutral-100 mb-4">System Health</h2>
				{loading ? (
					<p className="text-neutral-500">Loading...</p>
				) : diagnostics ? (
					<div className="grid gap-4 sm:grid-cols-3">
						<HealthItem title="Database" status={diagnostics.db.ok} />
						<HealthItem title="Redis" status={diagnostics.redis.ok} />
						<HealthItem title="Daemon" status={true} subtitle={diagnostics.daemon.url} />
					</div>
				) : (
					<p className="text-neutral-500">Failed to load diagnostics</p>
				)}
			</div>

			{/* Quick Actions */}
			<div className="bg-white dark:bg-neutral-800 rounded-lg border border-neutral-200 dark:border-neutral-700 p-6">
				<h2 className="text-lg font-medium text-neutral-900 dark:text-neutral-100 mb-4">Quick Actions</h2>
				<div className="grid gap-3 sm:grid-cols-2">
					<QuickAction title="Manage Users" description="View, edit, and manage user accounts" href="/admin/users" />
					<QuickAction title="System Settings" description="Configure system-wide settings" href="/admin/settings" />
				</div>
			</div>
		</div>
	)
}

function StatCard({ title, value, icon: Icon, href }: { title: string; value: string; icon: any; href: string }) {
	return (
		<Link
			to={href}
			className="bg-white dark:bg-neutral-800 rounded-lg border border-neutral-200 dark:border-neutral-700 p-4 hover:border-neutral-300 dark:hover:border-neutral-600 transition-colors group"
		>
			<div className="flex items-center justify-between">
				<div>
					<p className="text-sm text-neutral-500 dark:text-neutral-400">{title}</p>
					<p className="text-2xl font-semibold text-neutral-900 dark:text-neutral-100 mt-1">{value}</p>
				</div>
				<Icon className="h-8 w-8 text-neutral-300 dark:text-neutral-600 group-hover:text-neutral-400 dark:group-hover:text-neutral-500 transition-colors" />
			</div>
		</Link>
	)
}

function HealthItem({ title, status, subtitle }: { title: string; status: boolean; subtitle?: string }) {
	return (
		<div className="flex items-center gap-3 p-3 rounded-lg bg-neutral-50 dark:bg-neutral-900">
			<span className={cn('w-3 h-3 rounded-full', status ? 'bg-green-500' : 'bg-red-500')} />
			<div>
				<p className="text-sm font-medium text-neutral-900 dark:text-neutral-100">{title}</p>
				{subtitle && <p className="text-xs text-neutral-500 dark:text-neutral-400 truncate">{subtitle}</p>}
			</div>
		</div>
	)
}

function QuickAction({ title, description, href }: { title: string; description: string; href: string }) {
	return (
		<Link
			to={href}
			className="flex items-center justify-between p-4 rounded-lg border border-neutral-200 dark:border-neutral-700 hover:bg-neutral-50 dark:hover:bg-neutral-900 transition-colors group"
		>
			<div>
				<p className="font-medium text-neutral-900 dark:text-neutral-100">{title}</p>
				<p className="text-sm text-neutral-500 dark:text-neutral-400">{description}</p>
			</div>
			<ArrowRightIcon className="h-5 w-5 text-neutral-400 group-hover:text-neutral-600 dark:group-hover:text-neutral-300 transition-colors" />
		</Link>
	)
}
