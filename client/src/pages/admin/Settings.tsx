import React, { useState, useEffect, useCallback } from 'react'
import { useAuth } from '@/providers/AuthProvider'
import { useApi } from '@/api/client'
import { useAlert } from '@/components/ui/Alert'
import Button from '@/components/ui/Button'
import { cn } from '@/utils/cn'
import {
	CheckCircleIcon,
	XCircleIcon,
	ArrowPathIcon,
} from '@heroicons/react/24/outline'

export function AdminSettings() {
	const { token } = useAuth()
	const getTokenFn = useCallback(() => token, [token])
	const api = useApi(getTokenFn)
	const { notify } = useAlert()

	const [diagnostics, setDiagnostics] = useState<{
		db: { ok: boolean }
		redis: { ok: boolean }
		daemon: { url: string }
	} | null>(null)
	const [loading, setLoading] = useState(true)
	const [refreshing, setRefreshing] = useState(false)

	const fetchDiagnostics = useCallback(async () => {
		try {
			const res = await api.admin.diagnostics()
			setDiagnostics(res)
		} catch (e: any) {
			notify({ type: 'error', title: 'Failed to load diagnostics', description: e?.message })
		} finally {
			setLoading(false)
			setRefreshing(false)
		}
	}, [api, notify])

	useEffect(() => {
		fetchDiagnostics()
	}, [fetchDiagnostics])

	const handleRefresh = () => {
		setRefreshing(true)
		fetchDiagnostics()
	}

	return (
		<div className="space-y-6">
			<div className="flex items-center justify-between">
				<div>
					<h1 className="text-2xl font-semibold text-neutral-900 dark:text-neutral-100">Settings</h1>
					<p className="text-sm text-neutral-500 dark:text-neutral-400 mt-1">System configuration and diagnostics</p>
				</div>
				<Button
					variant="ghost"
					size="sm"
					onClick={handleRefresh}
					isLoading={refreshing}
				>
					<ArrowPathIcon className={cn('h-4 w-4 mr-2', refreshing && 'animate-spin')} />
					Refresh
				</Button>
			</div>

			{/* System Health */}
			<div className="bg-white dark:bg-neutral-800 rounded-lg border border-neutral-200 dark:border-neutral-700">
				<div className="px-6 py-4 border-b border-neutral-200 dark:border-neutral-700">
					<h2 className="text-lg font-medium text-neutral-900 dark:text-neutral-100">System Health</h2>
					<p className="text-sm text-neutral-500 dark:text-neutral-400">Status of connected services</p>
				</div>
				<div className="p-6">
					{loading ? (
						<p className="text-neutral-500">Loading...</p>
					) : diagnostics ? (
						<div className="space-y-4">
							<ServiceStatus
								name="MongoDB Database"
								status={diagnostics.db.ok}
								description="Primary data storage"
							/>
							<ServiceStatus
								name="Redis Cache"
								status={diagnostics.redis.ok}
								description="Session storage and caching"
							/>
							<ServiceStatus
								name="Daemon Service"
								status={true}
								description={diagnostics.daemon.url}
							/>
						</div>
					) : (
						<p className="text-neutral-500">Failed to load diagnostics</p>
					)}
				</div>
			</div>

			{/* System Info */}
			<div className="bg-white dark:bg-neutral-800 rounded-lg border border-neutral-200 dark:border-neutral-700">
				<div className="px-6 py-4 border-b border-neutral-200 dark:border-neutral-700">
					<h2 className="text-lg font-medium text-neutral-900 dark:text-neutral-100">System Information</h2>
				</div>
				<div className="p-6">
					<dl className="grid gap-4 sm:grid-cols-2">
						<InfoItem label="Version" value="1.0.0" />
						<InfoItem label="Environment" value="Production" />
						<InfoItem label="API Version" value="v1" />
						<InfoItem label="Node" value="Primary" />
					</dl>
				</div>
			</div>
		</div>
	)
}

function ServiceStatus({ name, status, description }: { name: string; status: boolean; description: string }) {
	return (
		<div className="flex items-center justify-between p-4 rounded-lg bg-neutral-50 dark:bg-neutral-900">
			<div className="flex items-center gap-3">
				{status ? (
					<CheckCircleIcon className="h-6 w-6 text-green-500" />
				) : (
					<XCircleIcon className="h-6 w-6 text-red-500" />
				)}
				<div>
					<p className="font-medium text-neutral-900 dark:text-neutral-100">{name}</p>
					<p className="text-sm text-neutral-500 dark:text-neutral-400">{description}</p>
				</div>
			</div>
			<span
				className={cn(
					'px-2 py-1 rounded-full text-xs font-medium',
					status
						? 'bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-400'
						: 'bg-red-100 text-red-800 dark:bg-red-900/30 dark:text-red-400'
				)}
			>
				{status ? 'Healthy' : 'Unhealthy'}
			</span>
		</div>
	)
}

function InfoItem({ label, value }: { label: string; value: string }) {
	return (
		<div className="p-3 rounded-lg bg-neutral-50 dark:bg-neutral-900">
			<dt className="text-xs text-neutral-500 dark:text-neutral-400 uppercase tracking-wider">{label}</dt>
			<dd className="mt-1 text-sm font-medium text-neutral-900 dark:text-neutral-100">{value}</dd>
		</div>
	)
}
