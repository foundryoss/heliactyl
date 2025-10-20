import React, { useEffect, useMemo, useState } from 'react'
import { useAuth } from '@/providers/AuthProvider'
import { Link, useLocation, useNavigate } from 'react-router-dom'
import { Button } from '@/components/ui/Button'
import { Input } from '@/components/ui/Input'
import { Label } from '@/components/ui/Label'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/Card'
import { Muted } from '@/components/ui/Typography'
import { useAlert } from '@/components/ui/Alert'
import { LockClosedIcon, UserPlusIcon } from '@heroicons/react/24/solid'

type Mode = 'login' | 'register'

export function AuthPage() {
    const location = useLocation()
    const nav = useNavigate()
    const modeFromPath = useMemo<Mode>(() => (location.pathname.endsWith('register') ? 'register' : 'login'), [location.pathname])
    const [mode, setMode] = useState<Mode>(modeFromPath)
    useEffect(() => setMode(modeFromPath), [modeFromPath])

    const { isAuthenticated, login, register } = useAuth()
    const { notify } = useAlert()

    useEffect(() => {
        if (isAuthenticated) nav('/')
    }, [isAuthenticated])

    // Shared state
    const [loading, setLoading] = useState(false)
    const [error, setError] = useState<string | null>(null)

    // Login state
    const [identifier, setIdentifier] = useState('')
    const [password, setPassword] = useState('')

    // Register state
    const [email, setEmail] = useState('')
    const [username, setUsername] = useState('')

    async function handleSubmit(e: React.FormEvent) {
        e.preventDefault()
        setLoading(true)
        setError(null)
        try {
            if (mode === 'login') {
                await login(identifier, password)
                notify({ type: 'success', title: 'Authentication successful', description: 'Good to see you again!' })
            } else {
                if ((password || '').length < 8) {
                    setError('Password must be at least 8 characters')
                    setLoading(false)
                    return
                }
                await register(email, username, password)
            }
            nav('/')
        } catch (e: any) {
            const message = e?.message || (mode === 'login' ? 'Login failed' : 'Registration failed')
            setError(message)
            notify({ type: 'error', title: mode === 'login' ? 'Login failed' : 'Registration failed', description: message })
        } finally {
            setLoading(false)
        }
    }

    const switchUrl = mode === 'login' ? '/register' : '/login'
    const switchLabel = mode === 'login' ? 'Create account' : 'Login'

    return (
        <div className="grid min-h-screen grid-cols-1 lg:grid-cols-2 bg-neutral-50 dark:bg-neutral-950">
            {/* Left visual panel */}
            <div className="relative hidden overflow-hidden lg:block bg-neutral-900">
                {/* Bottom fade */}
                <div className="absolute inset-0 pointer-events-none bg-gradient-to-b from-transparent to-neutral-950" />

                {/* Logo top-left */}
                <div className="absolute left-8 top-8 inline-flex items-center z-10">
                    <img src="https://i.ibb.co/SwyZ4Vpb/Uf0u-M26-1.png" alt="Altare Logo" className="h-8" />
                </div>

                {/* Bottom copy/links */}
                <div className="absolute inset-x-0 bottom-8 mx-8 z-10">
                    <div className="max-w-md rounded-xl bg-white/10 p-4 backdrop-blur-md">
                        <div className="text-sm font-semibold text-white">The world's largest free 24/7 hosting</div>
                        <div className="mt-1 text-xs text-neutral-400">Deploy, monitor, and scale your servers from a single dashboard.</div>
                        <div className="mt-3 flex flex-wrap gap-3 text-xs font-medium">
                            <Link to="/" className="text-white underline underline-offset-4">Home</Link>
                            <Link to={switchUrl} className="text-white underline underline-offset-4">{switchLabel}</Link>
                            <a href="https://docs.example.com" target="_blank" rel="noreferrer" className="text-white underline underline-offset-4">Docs</a>
                        </div>
                    </div>
                </div>
            </div>

            {/* Right auth panel */}
            <div className="flex items-center justify-center px-6 py-10 lg:px-12 bg-neutral-50 dark:bg-neutral-950">
                <div className="w-full max-w-md">
                    <div className="p-0.5 bg-neutral-200/50 dark:bg-neutral-800/50 rounded-lg">
                    <div className="px-2 py-2 flex items-center gap-2">
                        <LockClosedIcon className="h-4 w-4 text-neutral-600/30 dark:text-neutral-400/30" />
                        <span className="text-xs text-neutral-700 dark:text-neutral-300 font-semibold">Authentication required</span>
                    </div>
                    <Card>
                        <CardHeader>
                            {mode === 'login' ? (
                                <>
                                    <CardTitle>Welcome home</CardTitle>
                                    <CardDescription>Sign into your account</CardDescription>
                                </>
                            ) : (
                                <>
                                    <CardTitle>Create an account</CardTitle>
                                    <CardDescription>Join the 52,000+ users already using Altare</CardDescription>
                                </>
                            )}
                        </CardHeader>
                        <CardContent>
                            <form onSubmit={handleSubmit} className="space-y-4">
                                {mode === 'login' ? (
                                    <>
                                        <div>
                                            <Label requiredMark>Email</Label>
                                            <Input value={identifier} onChange={(e) => setIdentifier(e.target.value)} required />
                                        </div>
                                        <div>
                                            <Label requiredMark>Password</Label>
                                            <Input type="password" value={password} onChange={(e) => setPassword(e.target.value)} required />
                                        </div>
                                    </>
                                ) : (
                                    <>
                                        <div>
                                            <Label requiredMark>Email</Label>
                                            <Input type="email" value={email} onChange={(e) => setEmail(e.target.value)} required />
                                        </div>
                                        <div>
                                            <Label requiredMark>Username</Label>
                                            <Input value={username} onChange={(e) => setUsername(e.target.value)} required />
                                        </div>
                                        <div>
                                            <Label requiredMark>Password</Label>
                                            <Input type="password" value={password} onChange={(e) => setPassword(e.target.value)} required />
                                        </div>
                                    </>
                                )}

                                <Button type="submit" isLoading={loading} busyText="..." variant="primary" className="w-full">
                                    {mode === 'login' ? 'Login' : 'Register'}
                                </Button>
                            </form>
                            <Muted className="mt-4">
                                {mode === 'login' ? (
                                    <>No account? <Link to="/register" className="text-sky-600 underline underline-offset-2">Register</Link></>
                                ) : (
                                    <>Have an account? <Link to="/login" className="text-sky-600 underline underline-offset-2">Login</Link></>
                                )}
                            </Muted>
                        </CardContent>
                    </Card>
                    </div>
                    {/* Footer */}
                    <div className="mt-6 text-center text-xs text-gray-500">
                        <div>© 2024 - 2025 Nadhi.dev & Altare Technologies Inc.</div>
                        <div className="mt-1 relative inline-grid place-items-center whitespace-nowrap leading-none group">
                            {/* Invisible sizer to reserve space for full text so it doesn't wrap or jump */}
                            <span className="invisible">Powered by Heliactyl-RS Next 15 (Manhattan)</span>
                            {/* Default, short label */}
                            <span className="col-start-1 row-start-1 z-0 blur-0 transition duration-300 group-hover:opacity-0 group-hover:blur-sm">Powered by Heliactyl</span>
                            {/* Hover, full label */}
                            <span className="col-start-1 row-start-1 z-10 opacity-0 filter-none transition-opacity duration-300 group-hover:opacity-100">
                                Powered by Heliactyl-RS Next 15 (Manhattan)
                            </span>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    )
}

export default AuthPage


