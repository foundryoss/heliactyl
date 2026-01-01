import React from 'react'
import { Link, Outlet, useLocation, Navigate } from 'react-router-dom'
import { useAuth } from '@/providers/AuthProvider'
import { useTheme } from '@/providers/ThemeProvider'
import { cn } from '@/utils/cn'
import {
	UsersIcon,
	Cog6ToothIcon,
	ArrowLeftIcon,
	ChartBarIcon,
	ShieldCheckIcon,
	SunIcon,
	MoonIcon,
	ComputerDesktopIcon,
	ServerStackIcon,
	CodeBracketIcon,
} from '@heroicons/react/24/outline'

export function AdminLayout() {
	const { user } = useAuth()
	const { pathname } = useLocation()

	if (!user?.isAdmin) {
		return <Navigate to="/" replace />
	}

	const navItems = [
		{ path: '/admin', label: 'Overview', icon: ChartBarIcon },
		{ path: '/admin/users', label: 'Users', icon: UsersIcon },
		{ path: '/admin/nodes', label: 'Nodes', icon: ServerStackIcon },
		{ path: '/admin/software', label: 'Software', icon: CodeBracketIcon },
		{ path: '/admin/settings', label: 'Settings', icon: Cog6ToothIcon },
	]

	const linkClass = (path: string) => {
		const isActive = pathname === path
		return cn(
			'group inline-flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-xs transition-colors cursor-pointer border focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-neutral-900 dark:focus-visible:ring-neutral-100 focus-visible:ring-offset-2 focus-visible:ring-offset-white dark:focus-visible:ring-offset-neutral-950',
			isActive
				? 'font-semibold text-neutral-900 dark:text-neutral-100 bg-white dark:bg-neutral-700/90 border-neutral-200 dark:border-transparent shadow-xs'
				: 'font-medium text-neutral-700 dark:text-neutral-400 border-transparent dark:border-transparent hover:text-neutral-900 dark:hover:text-neutral-200 hover:bg-neutral-100 dark:hover:bg-neutral-800/70 hover:border-neutral-200 dark:hover:border-transparent'
		)
	}

	return (
		<div className="flex min-h-screen">
			{/* Admin Sidebar - matching main sidebar style */}
			<aside className="fixed inset-y-0 left-0 z-40 w-64">
				<div className="flex h-full flex-col px-2 py-3">
					<div className="mb-2 px-1.5 flex items-center gap-2">
						<Link to="/">
							<img src="https://i.ibb.co/CKvj0n7b/modryth-19.png" alt="Altare Logo" className="h-5 invert dark:invert-0" />
						</Link>
						<div className="ml-auto flex items-center gap-1">
							<ShieldCheckIcon className="h-4 w-4 text-yellow-500" />
							<span className="text-[10px] font-medium text-neutral-500 dark:text-neutral-400 uppercase tracking-wider">Admin</span>
						</div>
					</div>

					<nav className="mt-1 relative space-y-0.5">
						<span className="text-[10px] font-medium text-neutral-500 tracking-widest pb-2 ml-2 dark:text-neutral-400" style={{ fontFamily: 'Space Mono, sans-serif' }}>ADMIN PANEL</span>
						<div className="w-full mt-1">
							{navItems.map((item) => (
								<Link key={item.path} to={item.path} className={linkClass(item.path)}>
									<item.icon strokeWidth={2} className="h-4 w-4" />
									<span>{item.label}</span>
								</Link>
							))}
						</div>

						<span className="text-[10px] font-medium text-neutral-500 tracking-widest pb-2 dark:text-neutral-400 ml-2 mt-2 block pt-2" style={{ fontFamily: 'Space Mono, sans-serif' }}>NAVIGATION</span>
						<div className="w-full mt-1">
							<Link to="/" className={linkClass('/back')}>
								<ArrowLeftIcon strokeWidth={2} className="h-4 w-4" />
								<span>Back to Dashboard</span>
							</Link>
						</div>
					</nav>

					<div className="mt-auto space-y-1">
						<ThemeToggle />
					</div>
				</div>
			</aside>

			{/* Main Content */}
			<main className="ml-64 flex-1 min-h-screen transition-all duration-300 ease-out">
				<div className="mx-auto bg-neutral-50/90 dark:bg-gradient-to-t dark:from-neutral-900 dark:to-neutral-900 m-1 rounded-lg border border-neutral-200/50 dark:border-transparent p-6 min-h-screen">
					<Outlet />
				</div>
			</main>
		</div>
	)
}

function ThemeToggle() {
	const { theme, setTheme } = useTheme()
	const options: Array<{ value: typeof theme; icon: React.ReactNode }> = [
		{ value: 'light', icon: <SunIcon className="h-3 w-3" /> },
		{ value: 'dark', icon: <MoonIcon className="h-3 w-3" /> },
		{ value: 'system', icon: <ComputerDesktopIcon className="h-3 w-3" /> },
	]

	return (
		<div className="inline-flex items-center rounded-full ml-1 bg-neutral-200 dark:bg-neutral-800 p-0.5">
			{options.map(option => (
				<button
					key={option.value}
					type="button"
					onClick={() => setTheme(option.value as typeof theme)}
					className={cn(
						'flex items-center justify-center h-6 w-6 rounded-full cursor-pointer transition-colors',
						theme === option.value
							? 'bg-white dark:bg-neutral-700 text-neutral-900 dark:text-neutral-100 shadow-sm'
							: 'text-neutral-600 dark:text-neutral-400 hover:text-neutral-900 dark:hover:text-neutral-100'
					)}
				>
					{option.icon}
				</button>
			))}
		</div>
	)
}
