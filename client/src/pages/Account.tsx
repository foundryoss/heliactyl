import React, { useCallback, useEffect, useState } from 'react'
import { useAuth } from '@/providers/AuthProvider'
import { useApi } from '@/api/client'
import { Button } from '@/components/ui/Button'
import { Input } from '@/components/ui/Input'
import { Label } from '@/components/ui/Label'
import { Card, CardContent, CardFooter, CardHeader, CardTitle } from '@/components/ui/Card'
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/Tabs'
import { TrashIcon, ShieldCheckIcon, DevicePhoneMobileIcon, ClockIcon, GlobeAltIcon, ComputerDesktopIcon, CheckBadgeIcon, ExclamationTriangleIcon } from '@heroicons/react/24/outline'
import { useAlert } from '@/components/ui/Alert'
import { cn } from '@/utils/cn'

export function AccountPage() {
    const { token, user, refreshUser } = useAuth()
    const getTokenFn = useCallback(() => token, [token])
    const api = useApi(getTokenFn)

	const [email, setEmail] = useState(user?.email || '')
	const [username, setUsername] = useState(user?.username || '')
	const [pwdCur, setPwdCur] = useState('')
	const [pwdNew, setPwdNew] = useState('')
	const [sessions, setSessions] = useState<any[]>([])
	const [audit, setAudit] = useState<any[]>([])
	const [auditPage, setAuditPage] = useState(1)
	const [auditTotal, setAuditTotal] = useState(0)
	const [auditLoading, setAuditLoading] = useState(false)
    const { notify } = useAlert()

	useEffect(() => {
		api.authSessions.list(1, 50).then((r) => setSessions(r.items || [])).catch(() => {})
		loadAuditPage(1)
	}, [api])

	const loadAuditPage = async (page: number) => {
		setAuditLoading(true)
		try {
			const r = await api.account.audit(page, 10)
			setAudit(r.items || [])
			setAuditTotal(r.meta?.total || 0)
			setAuditPage(page)
		} catch (e) {
			console.error('Failed to load audit log:', e)
		} finally {
			setAuditLoading(false)
		}
	}

	const formatTimestamp = useCallback((timestamp: string | Date) => {
		const date = typeof timestamp === 'string' ? new Date(timestamp) : timestamp
		if (Number.isNaN(date.getTime())) return String(timestamp)
		const diffMs = Date.now() - date.getTime()
		const diffSeconds = Math.floor(diffMs / 1000)
		if (diffSeconds < 45) return 'just now'
		if (diffSeconds < 90) return '1 min ago'
		const diffMinutes = Math.floor(diffSeconds / 60)
		if (diffMinutes < 60) return `${diffMinutes} min${diffMinutes === 1 ? '' : 's'} ago`
		const diffHours = Math.floor(diffMinutes / 60)
		if (diffHours < 24) return `${diffHours} hour${diffHours === 1 ? '' : 's'} ago`
		const diffDays = Math.floor(diffHours / 24)
		if (diffDays < 7) return `${diffDays} day${diffDays === 1 ? '' : 's'} ago`
		return date.toLocaleString()
	}, [])

	const formatActionLabel = useCallback((action: string) => {
		const actionMap: Record<string, string> = {
			'auth:success': 'Signed in',
			'auth:fail': 'Sign-in failed',
			'auth:logout': 'Signed out',
			'auth:register': 'Account created',
			'account:update': 'Account updated',
			'session:revoke': 'Session revoked',
			'session:issue': 'Session issued',
		}
		if (actionMap[action]) return actionMap[action]
		return action.replace(':', ' · ')
	}, [])

	const summarizeMetadata = useCallback((metadata?: Record<string, unknown> | null) => {
		if (!metadata) return null
		const entries = Object.entries(metadata)
		if (!entries.length) return null
		return entries
			.map(([key, value]) => `${key}: ${typeof value === 'object' ? JSON.stringify(value) : String(value)}`)
			.join(' · ')
	}, [])

	async function saveProfile(e: React.FormEvent) {
		e.preventDefault()
		await api.account.update({ email, username })
		await refreshUser()
        notify({ type: 'success', title: 'Profile updated' })
	}

	async function changePassword(e: React.FormEvent) {
		e.preventDefault()
		await api.account.changePassword({ currentPassword: pwdCur, newPassword: pwdNew })
		setPwdCur(''); setPwdNew('')
        notify({ type: 'success', title: 'Password changed' })
	}

	async function revokeSession(id: string) {
		await api.authSessions.delete(id)
		setSessions((s) => s.filter((x) => x.id !== id))
        notify({ type: 'success', title: 'Session revoked' })
	}

	async function deleteAccount() {
		if (!confirm('Really delete your account?')) return
		await api.account.delete()
		window.location.href = '/login'
	}

    const formatDate = (dateString: string) => {
        try {
            return new Date(dateString).toLocaleString('en-US', {
                month: 'short',
                day: 'numeric',
                year: 'numeric',
                hour: '2-digit',
                minute: '2-digit'
            })
        } catch {
            return dateString
        }
    }

    const getDeviceIcon = (userAgent?: string) => {
        if (!userAgent) return <ComputerDesktopIcon className="h-4 w-4" />
        if (userAgent.includes('Mobile')) return <DevicePhoneMobileIcon className="h-4 w-4" />
        return <ComputerDesktopIcon className="h-4 w-4" />
    }

    const getBrowserInfo = (userAgent?: string) => {
        if (!userAgent) return 'Unknown Browser'
        if (userAgent.includes('Chrome')) return 'Chrome'
        if (userAgent.includes('Firefox')) return 'Firefox'
        if (userAgent.includes('Safari')) return 'Safari'
        if (userAgent.includes('Edge')) return 'Edge'
        return 'Unknown Browser'
    }

	const auditPageSize = 10
	const totalPages = Math.max(1, Math.ceil(auditTotal / auditPageSize) || 1)
	const hasPagination = auditTotal > auditPageSize

    const maskedEmail = React.useMemo(() => {
        const emailStr = user?.email || ''
        if (!emailStr) return ''
        const [name, domain] = emailStr.split('@')
        if (!name || !domain) return emailStr
        const visible = name.slice(0, 2)
        return `${visible}${'*'.repeat(Math.max(3, name.length - 2))}@${domain}`
    }, [user?.email])

    return (
        <div className="min-h-screen">
            {/* Header with user info */}
            <div>
                <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
                    <div className="pt-6">
                        <div className="flex items-center space-x-4">
                            <div className="w-14 h-14 rounded-2xl bg-yellow-100/50 shadow-xs border border-neutral-200/50 dark:border-neutral-700/50 dark:bg-yellow-950/30 flex items-center justify-center flex-shrink-0">
                                <span className="text-xl text-yellow-900 dark:text-yellow-300 font-bold uppercase">{user?.username?.charAt(0) || 'U'}</span>
                            </div>
                            <div className="flex-1 min-w-0">
                                <h1 className="text-2xl font-semibold text-neutral-900 dark:text-neutral-100">{user?.username || 'User'}</h1>
                                <div className="flex items-center space-x-4 mt-1">
                                    <p className="text-sm text-neutral-500 dark:text-neutral-400">{user?.email}</p>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
            </div>

            {/* Main content */}
            <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
					<Tabs defaultValue="profile" className="w-full">
						<div className="mb-4">
							<TabsList className="inline-flex items-center rounded-full bg-neutral-200 dark:bg-neutral-800 p-0.5 h-auto">
								<TabsTrigger 
									value="profile" 
									className="data-[state=active]:bg-white dark:data-[state=active]:bg-neutral-700 data-[state=active]:text-neutral-900 dark:data-[state=active]:text-neutral-100 text-neutral-600 dark:text-neutral-400 hover:text-neutral-900 dark:hover:text-neutral-100 rounded-full px-3 h-7 text-xs font-semibold transition-colors"
								>
									Overview
								</TabsTrigger>
								<TabsTrigger 
									value="security" 
									className="data-[state=active]:bg-white dark:data-[state=active]:bg-neutral-700 data-[state=active]:text-neutral-900 dark:data-[state=active]:text-neutral-100 text-neutral-600 dark:text-neutral-400 hover:text-neutral-900 dark:hover:text-neutral-100 rounded-full px-3 h-7 text-xs font-semibold transition-colors"
								>
									Change password
								</TabsTrigger>
								<TabsTrigger 
									value="sessions" 
									className="data-[state=active]:bg-white dark:data-[state=active]:bg-neutral-700 data-[state=active]:text-neutral-900 dark:data-[state=active]:text-neutral-100 text-neutral-600 dark:text-neutral-400 hover:text-neutral-900 dark:hover:text-neutral-100 rounded-full px-3 h-7 text-xs font-semibold transition-colors"
								>
									Active sessions
								</TabsTrigger>
								<TabsTrigger 
									value="logs" 
									className="data-[state=active]:bg-white dark:data-[state=active]:bg-neutral-700 data-[state=active]:text-neutral-900 dark:data-[state=active]:text-neutral-100 text-neutral-600 dark:text-neutral-400 hover:text-neutral-900 dark:hover:text-neutral-100 rounded-full px-3 h-7 text-xs font-semibold transition-colors"
								>
									Audit logs
								</TabsTrigger>
								{/* 
								<TabsTrigger 
									value="danger" 
									className="hidden data-[state=active]:bg-white data-[state=active]:text-red-700 data-[state=active]:shadow-xs text-gray-600 hover:text-gray-900 rounded-full px-3 h-7 text-xs font-semibold transition-colors"
								>
									Danger zone
								</TabsTrigger> */}
							</TabsList>
						</div>
                    
                    <TabsContent value="profile" className="mt-0">
                        <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
                            <div className="lg:col-span-2">
                                <Card className="">
                                    <CardHeader>
                                        <CardTitle className="text-lg font-semibold">Update your profile</CardTitle>
                                        <p className="text-sm text-neutral-600 dark:text-neutral-400">Change your username and email address.</p>
                                    </CardHeader>
                                    <CardContent>
                                        <form onSubmit={saveProfile} className="space-y-6">
                                            <div className="grid grid-cols-1 sm:grid-cols-2 gap-6">
                                                <div>
                                                    <Label htmlFor="username" className="text-sm font-semibold text-neutral-700 dark:text-neutral-300">Username</Label>
                                                    <Input 
                                                        id="username"
                                                        value={username || ''} 
                                                        onChange={(e) => setUsername(e.target.value)}
                                                        className="mt-1"
                                                        placeholder="Enter your username"
                                                    />
                                                </div>
                                                <div>
                                                    <Label htmlFor="email" className="text-sm font-semibold text-neutral-700 dark:text-neutral-300">Email address</Label>
                                                    <Input 
                                                        id="email"
                                                        type="email"
                                                        value={email} 
                                                        onChange={(e) => setEmail(e.target.value)}
                                                        className="mt-1"
                                                        placeholder="Enter your email"
                                                    />
                                                </div>
                                            </div>
                                            <div className="flex justify-end">
                                                <Button type="submit" variant="primary" size="md">
                                                    Save Changes
                                                </Button>
                                            </div>
                                        </form>
                                    </CardContent>
                                </Card>
                            </div>
                        </div>
                    </TabsContent>
                    
                    <TabsContent value="security" className="mt-0">
                        <div className="max-w-2xl">
                            <Card className="">
                                <CardHeader>
                                    <CardTitle className="text-lg font-semibold">Change Password</CardTitle>
                                    <p className="text-sm text-neutral-600 dark:text-neutral-400">Ensure your account is using a long, random password to stay secure.</p>
                                </CardHeader>
                                <CardContent>
                                    <form onSubmit={changePassword} className="space-y-6">
                                        <div>
                                            <Label htmlFor="current-password" className="text-sm font-semibold text-neutral-700 dark:text-neutral-300">Current Password</Label>
                                            <Input 
                                                id="current-password"
                                                type="password" 
                                                value={pwdCur} 
                                                onChange={(e) => setPwdCur(e.target.value)}
                                                className="mt-1"
                                                placeholder="Enter your current password"
                                            />
                                        </div>
                                        <div>
                                            <Label htmlFor="new-password" className="text-sm font-semibold text-neutral-700 dark:text-neutral-300">New Password</Label>
                                            <Input 
                                                id="new-password"
                                                type="password" 
                                                value={pwdNew} 
                                                onChange={(e) => setPwdNew(e.target.value)}
                                                className="mt-1"
                                                placeholder="Enter your new password"
                                            />
                                            <p className="mt-1 text-xs text-neutral-500 dark:text-neutral-400">Password must be at least 8 characters long.</p>
                                        </div>
                                        <div className="flex justify-end">
                                            <Button type="submit" variant="primary" size="md">
                                                Update Password
                                            </Button>
                                        </div>
                                    </form>
                                </CardContent>
                            </Card>
                        </div>
                    </TabsContent>
                    
                    <TabsContent value="sessions" className="mt-0">
                        <Card className="">
                            <CardHeader>
                                <CardTitle className="text-lg font-semibold">Active sessions</CardTitle>
                                <p className="text-sm text-neutral-600 dark:text-neutral-400">Manage and monitor your active sessions on Altare across all devices.</p>
                            </CardHeader>
                            <CardContent>
                                <div className="space-y-4">
                                    {sessions.length === 0 ? (
                                        <div className="text-center py-12">
                                            <h3 className="mt-2 text-sm font-semibold text-neutral-900 dark:text-neutral-100">No active sessions</h3>
                                            <p className="mt-1 text-sm text-neutral-500 dark:text-neutral-400">You have no other active sessions.</p>
                                        </div>
                                    ) : (
                                        sessions.map((s) => (
                                            <div key={s.id} className="relative bg-white dark:bg-neutral-800 border border-neutral-200 dark:border-transparent rounded-lg p-6 hover:border-neutral-300 dark:hover:border-neutral-600 transition-colors">
                                                <div className="flex items-start justify-between">
                                                    <div className="flex items-start space-x-4">
                                                        <div className="flex-shrink-0">
                                                            <div className="w-10 h-10 bg-neutral-100 dark:bg-neutral-700 rounded-lg flex items-center justify-center">
                                                                {getDeviceIcon(s.userAgent)}
                                                            </div>
                                                        </div>
                                                        <div className="flex-1 min-w-0">
                                                            <div className="flex items-center space-x-2">
                                                                <h3 className="text-sm font-semibold text-neutral-900 dark:text-neutral-100">
                                                                    {getBrowserInfo(s.userAgent)}
                                                                </h3>
                                                                {s.isCurrent && (
                                                                    <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-semibold bg-green-100 text-green-800">
                                                                        <div className="w-1.5 h-1.5 bg-green-400 rounded-full mr-1.5"></div>
                                                                        This session
                                                                    </span>
                                                                )}
                                                            </div>
                                                            <div className="mt-1 flex items-center space-x-4 text-sm text-neutral-500 dark:text-neutral-400">
                                                                <div className="flex items-center space-x-1">
                                                                    <GlobeAltIcon className="w-4 h-4" />
                                                                    <span>{s.ip || 'Unknown IP'}</span>
                                                                </div>
                                                                <div className="flex items-center space-x-1">
                                                                    <ClockIcon className="w-4 h-4" />
                                                                    <span>Started {formatDate(s.createdAt)}</span>
                                                                </div>
                                                            </div>
                                                            {s.userAgent && (
                                                                <p className="mt-2 text-xs text-neutral-400 dark:text-neutral-500 truncate max-w-md">
                                                                    {s.userAgent}
                                                                </p>
                                                            )}
                                                        </div>
                                                    </div>
                                                    <div className="flex-shrink-0">
                                                        {!s.isCurrent && (
                                                            <Button 
                                                                variant="ghost" 
                                                                size="sm" 
                                                                onClick={() => revokeSession(s.id)}
                                                                className="text-red-600 bg-red-50 hover:text-red-700 hover:bg-red-100"
                                                            >
                                                                Revoke
                                                            </Button>
                                                        )}
                                                    </div>
                                                </div>
                                            </div>
                                        ))
                                    )}
                                </div>
                            </CardContent>
                        </Card>
                    </TabsContent>
                    
					<TabsContent value="logs" className="mt-0">
						<Card className="">
							<CardHeader>
								<CardTitle className="text-lg font-semibold">Recent activity</CardTitle>
								<p className="text-sm text-neutral-600 dark:text-neutral-400">An activity log of recent actions</p>
							</CardHeader>
							<CardContent>
								<div className="space-y-4">
									{auditLoading && audit.length === 0 ? (
										<div className="relative">
											<div className="space-y-3">
												{(() => { const skLen = 5; return Array.from({ length: skLen }).map((_, idx) => (
													<div key={idx} className="timeline-item relative flex gap-3">
														<div className="flex-1 rounded-md border border-neutral-200/80 dark:border-neutral-700/60 bg-white/60 dark:bg-neutral-900/40 p-3">
															<div className="h-3 w-40 bg-neutral-200 dark:bg-neutral-700 rounded animate-pulse" />
															<div className="mt-2 h-2.5 w-60 bg-neutral-200 dark:bg-neutral-700 rounded animate-pulse" />
															<div className="mt-3 h-2 w-24 bg-neutral-200 dark:bg-neutral-700 rounded animate-pulse" />
														</div>
													</div>
												)) })()}
											</div>
										</div>
									) : audit.length === 0 ? (
										<div className="rounded-md border border-dashed border-neutral-200 dark:border-neutral-700 py-8 text-center">
											<p className="text-sm font-medium text-neutral-600 dark:text-neutral-300">No activity yet</p>
											<p className="text-xs text-neutral-500 dark:text-neutral-400 mt-1">Actions from you and other members will show up here.</p>
										</div>
									) : (
										<div className="relative">
											<div className="space-y-3">
												{audit.map((item: any) => {
													const formattedMeta = summarizeMetadata(item.metadata)
													return (
														<div key={item.id} className="timeline-item relative flex gap-3">
															<div className="flex-1 rounded-md border border-neutral-200/80 dark:border-neutral-700/60 bg-white/60 dark:bg-neutral-900/40 py-2 px-3 pt-3">
																<div className="flex items-center justify-between gap-3">
																	<div className="flex items-center gap-2 text-sm text-neutral-900 dark:text-neutral-100 font-medium">
																		<span>{formatActionLabel(item.action)}</span>
																	</div>
																	<span className="text-xs text-neutral-500 dark:text-neutral-400">{formatTimestamp(item.createdAt)}</span>
																</div>
																{formattedMeta && (
																	<pre style={{ fontFamily: 'Space Mono, sans-serif' }} className="mt-1 text-xs text-neutral-500 w-98 dark:text-neutral-400 bg-white shadow-xs border border-neutral-200/80 dark:border-transparent dark:bg-neutral-800/50 px-2 py-1 rounded-md mt-2">{formattedMeta}</pre>
																)}
																<div className="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-[10px] uppercase tracking-[0.3em] text-neutral-500 dark:text-neutral-400">
																	{item.ip && <span>IP {item.ip}</span>}
																	{item.userAgent && <span>{item.userAgent}</span>}
																</div>
															</div>
														</div>
													)
												})}
											</div>

											{hasPagination && (
												<div className="mt-4 flex items-center justify-center gap-2 text-xs text-neutral-500 dark:text-neutral-400">
													<Button
														variant="ghost"
														size="sm"
														className="h-7 w-7 rounded-full p-0"
														onClick={() => loadAuditPage(auditPage - 1)}
														disabled={auditPage === 1 || auditLoading}
														aria-label="Previous audit page"
													>
														<span className="sr-only">Previous</span>
														‹
													</Button>
													<span className="font-semibold text-neutral-700 dark:text-neutral-300">
														{auditPage} / {totalPages}
													</span>
													<Button
														variant="ghost"
														size="sm"
														className="h-7 w-7 rounded-full p-0"
														onClick={() => loadAuditPage(auditPage + 1)}
														disabled={auditPage * auditPageSize >= auditTotal || auditLoading}
														aria-label="Next audit page"
													>
														<span className="sr-only">Next</span>
														›
													</Button>
												</div>
											)}
											</div>
											)}
										</div>
									</CardContent>
						</Card>
					</TabsContent>
                    
                    <TabsContent value="danger" className="mt-0">
                        <div className="max-w-2xl">
                            <Card className=" border-red-200">
                                <CardHeader>
                                    <div className="flex items-center space-x-2">
                                        <ExclamationTriangleIcon className="h-5 w-5 text-red-600" />
                                        <CardTitle className="text-lg font-semibold text-red-700">Danger Zone</CardTitle>
                                    </div>
                                    <p className="text-sm text-red-600">Irreversible and destructive actions.</p>
                                </CardHeader>
                                <CardContent>
                                    <div className="bg-red-50 border border-red-200 rounded-lg p-6">
                                        <div className="flex items-start space-x-3">
                                            <ExclamationTriangleIcon className="h-6 w-6 text-red-600 mt-0.5" />
                                            <div className="flex-1">
                                                <h3 className="text-sm font-semibold text-red-800">Delete Account</h3>
                                                <div className="mt-2 text-sm text-red-700">
                                                    <p>Once you delete your account, there is no going back. This will:</p>
                                                    <ul className="list-disc list-inside mt-2 space-y-1">
                                                        <li>Permanently delete your profile and account data</li>
                                                        <li>Remove you from all tenants and organizations</li>
                                                        <li>Cancel any active subscriptions</li>
                                                        <li>Delete all your servers and configurations</li>
                                                    </ul>
                                                </div>
                                                <div className="mt-4">
                                                    <Button 
                                                        variant="destructive" 
                                                        onClick={deleteAccount} 
                                                        leftIcon={<TrashIcon className="h-4 w-4"/>}
                                                        size="md"
                                                    >
                                                        Delete Account Permanently
                                                    </Button>
                                                </div>
                                            </div>
                                        </div>
                                    </div>
                                </CardContent>
                            </Card>
                        </div>
                    </TabsContent>
                </Tabs>
            </div>
        </div>
    )
}

