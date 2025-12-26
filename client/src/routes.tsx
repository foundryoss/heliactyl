import { Navigate, Outlet, Route, Routes } from 'react-router-dom'
import { useAuth } from '@/providers/AuthProvider'
import { useSidebar } from '@/providers/SidebarProvider'
import { ErrorBoundary } from '@/app/error-boundary'
import AuthPage from '@/pages/Auth'
import { DashboardPage } from '@/pages/Dashboard'
import { AccountPage } from '@/pages/Account'
import { AdminPage } from '@/pages/Admin'
import { WalletPage } from '@/pages/Wallet'
import { BillingPage } from '@/pages/Billing'
import React, { useEffect } from 'react'
import { ServersPage } from '@/pages/Servers'
import { ServerRoutes } from '@/pages/server'
import { useTenants } from '@/providers/TenantProvider'
import { Sidebar } from '@/components/Sidebar'
import { SidebarGripHandle } from '@/components/FloatingSidebarToggle'
import { PageTransition } from '@/components/layout/PageTransition'
import { cn } from '@/utils/cn'

function Layout() {
    const { isAuthenticated } = useAuth()
    const { isCollapsed, toggleCollapsed } = useSidebar()
    const { refreshTenants } = useTenants()
    const [isMobile, setIsMobile] = React.useState(false)

    useEffect(() => {
        if (!isAuthenticated) return
        refreshTenants()
    }, [isAuthenticated])

    useEffect(() => {
        const checkMobile = () => {
            setIsMobile(window.innerWidth <= 768)
        }
        checkMobile()
        
        const handleResize = () => checkMobile()
        window.addEventListener('resize', handleResize)
        return () => window.removeEventListener('resize', handleResize)
    }, [])

    // Do not auto-navigate on tenant change; let user choose pages

    return (
        <ErrorBoundary>
            <div className="min-h-full">
                <Sidebar />
                <SidebarGripHandle />
                <main className={cn(
                    "min-h-screen transition-all duration-300 ease-out",
                    // On mobile, add top padding when sidebar is collapsed, otherwise use sidebar padding
                    isMobile 
                        ? (isCollapsed ? "pt-20 pl-5 pr-4" : "pl-0")
                        : (isCollapsed ? "pl-5" : "pl-64")
                )}>
				<div className="mx-auto bg-neutral-50/90 dark:bg-gradient-to-t dark:from-neutral-900 dark:to-neutral-900 m-1 rounded-lg border border-neutral-200/50 dark:border-transparent p-6 min-h-screen">
                        <PageTransition>
                            <Outlet />
                        </PageTransition>
                    </div>
                </main>
            </div>
        </ErrorBoundary>
    )
}

function PrivateRoute() {
	const { isAuthenticated } = useAuth()
	if (!isAuthenticated) return <Navigate to="/login" replace />
	return <Outlet />
}

export function AppRoutes() {
	return (
		<Routes>
            {/* Public auth routes (no app chrome, full-screen) */}
            <Route path="login" element={<AuthPage />} />
            <Route path="register" element={<AuthPage />} />

            {/* App routes with navbar/layout */}
            <Route element={<Layout />}>
				<Route element={<PrivateRoute />}>
					<Route index element={<DashboardPage />} />
                    <Route path="servers" element={<ServersPage />} />
                    <Route path="server/:serverId/*" element={<ServerRoutes />} />
                    <Route path="billing" element={<BillingPage />} />
                    <Route path="wallet" element={<WalletPage />} />
					<Route path="account" element={<AccountPage />} />
					<Route path="admin" element={<AdminPage />} />
				</Route>
				<Route path="*" element={<Navigate to="/" replace />} />
			</Route>
		</Routes>
	)
}

