import React from 'react'
import { createPortal } from 'react-dom'
import { Link, useLocation } from 'react-router-dom'
import { useAuth } from '@/providers/AuthProvider'
import { useTenants } from '@/providers/TenantProvider'
import { useTheme } from '@/providers/ThemeProvider'
import { useSidebar } from '@/providers/SidebarProvider'
import Button from '@/components/ui/Button'
import Modal from '@/components/ui/Modal'
import { useAlert } from '@/components/ui/Alert'
import Tooltip from '@/components/ui/Tooltip'
import { cn } from '@/utils/cn'
import { Input } from '@/components/ui/Input'
import Label from '@/components/ui/Label'
import { useApi } from '@/api/client'
import {
    HomeIcon,
    UserIcon,
    ServerStackIcon,
    BuildingOfficeIcon,
    Cog6ToothIcon,
    ArrowRightOnRectangleIcon,
    ChevronLeftIcon,
    PlusIcon,
    SunIcon,
    MoonIcon,
    ComputerDesktopIcon,
    ArrowsRightLeftIcon,
    WalletIcon,
    CurrencyDollarIcon
} from '@heroicons/react/24/outline'
import { ChevronRightIcon } from '@heroicons/react/16/solid'

// Custom Panel Icon based on the close sidebar icon
function PanelIcon({ className }: { className?: string }) {
    return (
        <svg width="20" height="20" viewBox="0 0 20 20" fill="currentColor" xmlns="http://www.w3.org/2000/svg" data-rtl-flip="" className={className}><path d="M6.83496 3.99992C6.38353 4.00411 6.01421 4.0122 5.69824 4.03801C5.31232 4.06954 5.03904 4.12266 4.82227 4.20012L4.62207 4.28606C4.18264 4.50996 3.81498 4.85035 3.55859 5.26848L3.45605 5.45207C3.33013 5.69922 3.25006 6.01354 3.20801 6.52824C3.16533 7.05065 3.16504 7.71885 3.16504 8.66301V11.3271C3.16504 12.2712 3.16533 12.9394 3.20801 13.4618C3.25006 13.9766 3.33013 14.2909 3.45605 14.538L3.55859 14.7216C3.81498 15.1397 4.18266 15.4801 4.62207 15.704L4.82227 15.79C5.03904 15.8674 5.31234 15.9205 5.69824 15.9521C6.01398 15.9779 6.383 15.986 6.83398 15.9902L6.83496 3.99992ZM18.165 11.3271C18.165 12.2493 18.1653 12.9811 18.1172 13.5702C18.0745 14.0924 17.9916 14.5472 17.8125 14.9648L17.7295 15.1415C17.394 15.8 16.8834 16.3511 16.2568 16.7353L15.9814 16.8896C15.5157 17.1268 15.0069 17.2285 14.4102 17.2773C13.821 17.3254 13.0893 17.3251 12.167 17.3251H7.83301C6.91071 17.3251 6.17898 17.3254 5.58984 17.2773C5.06757 17.2346 4.61294 17.1508 4.19531 16.9716L4.01855 16.8896C3.36014 16.5541 2.80898 16.0434 2.4248 15.4169L2.27051 15.1415C2.03328 14.6758 1.93158 14.167 1.88281 13.5702C1.83468 12.9811 1.83496 12.2493 1.83496 11.3271V8.66301C1.83496 7.74072 1.83468 7.00898 1.88281 6.41985C1.93157 5.82309 2.03329 5.31432 2.27051 4.84856L2.4248 4.57317C2.80898 3.94666 3.36012 3.436 4.01855 3.10051L4.19531 3.0175C4.61285 2.83843 5.06771 2.75548 5.58984 2.71281C6.17898 2.66468 6.91071 2.66496 7.83301 2.66496H12.167C13.0893 2.66496 13.821 2.66468 14.4102 2.71281C15.0069 2.76157 15.5157 2.86329 15.9814 3.10051L16.2568 3.25481C16.8833 3.63898 17.394 4.19012 17.7295 4.84856L17.8125 5.02531C17.9916 5.44285 18.0745 5.89771 18.1172 6.41985C18.1653 7.00898 18.165 7.74072 18.165 8.66301V11.3271ZM8.16406 15.995H12.167C13.1112 15.995 13.7794 15.9947 14.3018 15.9521C14.8164 15.91 15.1308 15.8299 15.3779 15.704L15.5615 15.6015C15.9797 15.3451 16.32 14.9774 16.5439 14.538L16.6299 14.3378C16.7074 14.121 16.7605 13.8478 16.792 13.4618C16.8347 12.9394 16.835 12.2712 16.835 11.3271V8.66301C16.835 7.71885 16.8347 7.05065 16.792 6.52824C16.7605 6.14232 16.7073 5.86904 16.6299 5.65227L16.5439 5.45207C16.32 5.01264 15.9796 4.64498 15.5615 4.3886L15.3779 4.28606C15.1308 4.16013 14.8165 4.08006 14.3018 4.03801C13.7794 3.99533 13.1112 3.99504 12.167 3.99504H8.16406C8.16407 3.99667 8.16504 3.99829 8.16504 3.99992L8.16406 15.995Z"></path></svg>
    )
}

export function Sidebar() {
    const { isAuthenticated, user, logout } = useAuth()
    const { pathname } = useLocation()
    const { isCollapsed, toggleCollapsed } = useSidebar()
    const [isMobile, setIsMobile] = React.useState(false)

    React.useEffect(() => {
        const checkMobile = () => {
            setIsMobile(window.innerWidth <= 768)
        }
        checkMobile()
        
        const handleResize = () => checkMobile()
        window.addEventListener('resize', handleResize)
        return () => window.removeEventListener('resize', handleResize)
    }, [])

    if (!isAuthenticated) return null

    const linkClass = (path: string) => {
        const isActive = pathname === path
        return cn(
            'group inline-flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-xs transition-colors cursor-pointer border focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-neutral-900 dark:focus-visible:ring-neutral-100 focus-visible:ring-offset-2 focus-visible:ring-offset-white dark:focus-visible:ring-offset-neutral-950',
            isActive
                ? 'font-semibold text-neutral-900 dark:text-neutral-100 bg-white dark:bg-neutral-700/90 border-neutral-200 dark:border-transparent shadow-xs'
                : 'font-medium text-neutral-700 dark:text-neutral-400 border-transparent dark:border-transparent hover:text-neutral-900 dark:hover:text-neutral-200 hover:bg-neutral-100 dark:hover:bg-neutral-800/70 hover:border-neutral-200 dark:hover:border-transparent'
        )
    }

    // Mobile: full overlay when open, hidden when closed
    // Desktop: fixed sidebar with width animation
    return (
        <>
            {/* Mobile backdrop */}
            {isMobile && !isCollapsed && (
                <div 
                    className="fixed inset-0 z-30 bg-black/50 backdrop-blur-sm"
                    onClick={toggleCollapsed}
                />
            )}
            
            <aside className={cn(
                "fixed inset-y-0 left-0 z-40 transition-all duration-300 ease-out",
                isMobile 
                    ? (isCollapsed ? "-translate-x-full" : "translate-x-0 w-64")
                    : (isCollapsed ? "w-0" : "w-64")
            )}>
                <div className={cn(
                    "flex h-full flex-col px-2 py-3 transition-opacity duration-300 ease-out",
                    // Mobile: always visible when not collapsed, Desktop: fade based on collapse
                    isMobile 
                        ? "opacity-100" 
                        : (isCollapsed ? "opacity-0 pointer-events-none" : "opacity-100")
                )}>
                    <div className="mb-2 px-1.5 flex items-center justify-between gap-2">
                        <Link to="/">
                            <img src="https://i.ibb.co/CKvj0n7b/modryth-19.png" alt="Altare Logo" className="h-5 invert dark:invert-0" />
                        </Link>
                        <Tooltip content="Close sidebar" placement="right">
                            <button
                                onClick={toggleCollapsed}
                                className="inline-flex items-center justify-center w-6 h-6 rounded-md text-neutral-600 dark:text-neutral-400 hover:text-neutral-900 dark:hover:text-neutral-100 hover:bg-neutral-100 dark:hover:bg-neutral-800 transition-colors focus:outline-none focus:ring-2 focus:ring-neutral-900 dark:focus:ring-neutral-100 focus:ring-offset-2 focus:ring-offset-white dark:focus:ring-offset-neutral-950"
                            >
                                <PanelIcon className="h-4 w-4" />
                            </button>
                        </Tooltip>
                    </div>

                    <nav className="mt-1 relative space-y-0.5">
                        <span className="text-[10px] font-medium text-neutral-500 tracking-widest pb-2 ml-2 dark:text-neutral-400" style={{ fontFamily: 'Space Mono, sans-serif' }}>HOSTING</span>
                        <div className="w-full mt-1">
                            <Link to="/" className={linkClass('/')}> <HomeIcon strokeWidth={2} className="h-4 w-4"/> <span>Dashboard</span></Link>
                        </div>
                        <Link to="/servers" className={linkClass('/servers')}> <ServerStackIcon strokeWidth={2} className="h-4 w-4"/> <span>Servers</span></Link>
                        {user?.isAdmin ? (
                            <Link to="/admin" className={linkClass('/admin')}> <Cog6ToothIcon strokeWidth={2} className="h-4 w-4"/> <span>Admin</span></Link>
                        ) : null}

                        <span className="text-[10px] font-medium text-neutral-500 tracking-widest pb-2 dark:text-neutral-400 ml-2 mt-2" style={{ fontFamily: 'Space Mono, sans-serif' }}>ECONOMY</span>
                        <div className="w-full mt-1">
                            <Link to="/billing" className={linkClass('/billing')}> <CurrencyDollarIcon strokeWidth={2} className="h-4 w-4"/> <span>Billing</span></Link>
                        </div>
                        <Link to="/wallet" className={linkClass('/wallet')}> <WalletIcon strokeWidth={2} className="h-4 w-4"/> <span>Wallet</span></Link>
                    </nav>

                    <div className="mt-auto space-y-1">
                        <ThemeToggle />
                        <UserDropdown />
                    </div>
                </div>
            </aside>
        </>
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

function UserDropdown() {
    const { user, logout } = useAuth()
    const [open, setOpen] = React.useState(false)
    const [tenantsOpen, setTenantsOpen] = React.useState(false)
    const [tenantsPosition, setTenantsPosition] = React.useState({ left: 0, bottom: 0 })
    const containerRef = React.useRef<HTMLDivElement | null>(null)
    const buttonRef = React.useRef<HTMLButtonElement | null>(null)
    const tenantsRef = React.useRef<HTMLDivElement | null>(null)
    const tenantsTriggerRef = React.useRef<HTMLButtonElement | null>(null)
    
    // Modal states for tenant actions
    const [membersState, setMembersState] = React.useState<{ open: boolean; tenantId: string | null; tenantName: string }>(() => ({ open: false, tenantId: null, tenantName: '' }))
    const [deleteState, setDeleteState] = React.useState<{ open: boolean; tenantId: string | null; tenantName: string }>(() => ({ open: false, tenantId: null, tenantName: '' }))
    const [createOpen, setCreateOpen] = React.useState(false)
    
    const maskedEmail = React.useMemo(() => {
        const email = user?.email || ''
        if (!email) return ''
        const [name, domain] = email.split('@')
        if (!name || !domain) return email
        const visible = name.slice(0, 2)
        return `${visible}${'*'.repeat(Math.max(3, name.length - 2))}@${domain}`.toUpperCase()
    }, [user?.email])

    React.useEffect(() => {
        function onDocClick(e: MouseEvent) {
            if (!open && !tenantsOpen) return
            const target = e.target as HTMLElement | null
            
            // Don't close if clicking on dropdown elements
            if (buttonRef.current && buttonRef.current.contains(target)) return
            if (containerRef.current && containerRef.current.contains(target)) return
            if (tenantsRef.current && tenantsRef.current.contains(target)) return
            
            // Don't close if clicking on any modal or portal element
            const modalElement = target?.closest('[role="dialog"], .fixed, #tenant-menu-popover')
            if (modalElement) return
            
            // Don't close if the target has a high z-index (likely a modal)
            const computedStyle = target ? window.getComputedStyle(target) : null
            const zIndex = computedStyle ? parseInt(computedStyle.zIndex) || 0 : 0
            if (zIndex > 50) return
            
            setOpen(false)
            setTenantsOpen(false)
        }
        document.addEventListener('mousedown', onDocClick, { capture: false })
        return () => document.removeEventListener('mousedown', onDocClick, { capture: false })
    }, [open, tenantsOpen])

    const handleClose = () => {
        setOpen(false)
        setTenantsOpen(false)
    }

    const handleTenantsClick = () => {
        if (tenantsTriggerRef.current) {
            const rect = tenantsTriggerRef.current.getBoundingClientRect()
            setTenantsPosition({
                left: rect.right + 4,
                bottom: window.innerHeight - rect.bottom
            })
        }
        setTenantsOpen(true)
    }

    return (
        <div className="relative">
            <button
                ref={buttonRef}
                onClick={() => setOpen(!open)}
                className={cn(
                    'flex w-full items-center rounded-md hover:shadow-xs border border-transparent hover:border-neutral-200 dark:hover:border-transparent dark:hover:shadow-none px-2 py-2 text-left text-xs text-neutral-800 dark:text-neutral-200 hover:bg-neutral-50 dark:hover:bg-neutral-800 transition-colors duration-150 cursor-pointer',
                    open && 'bg-neutral-50 dark:bg-neutral-800'
                )}
                aria-expanded={open}
            >
                <div className="inline-flex items-center gap-2 min-w-0">
                    <div className="w-5 h-5 rounded bg-yellow-500 border border-yellow-600/50 flex items-center justify-center flex-shrink-0">
                        <span className="text-xs text-yellow-800 font-medium uppercase">{user?.username?.charAt(0) || 'U'}</span>
                    </div>
                    <div className="flex flex-col min-w-0">
                        <span className="truncate text-xs font-medium">{user?.username || 'Account'}</span>
                        {maskedEmail && (
                            <span className="truncate text-[9px] text-neutral-500 dark:text-neutral-400 tracking-widest uppercase">{maskedEmail}</span>
                        )}
                    </div>
                </div>
            </button>

            {open && (
                <div
                    ref={containerRef}
                    className="absolute bottom-full left-0 right-0 mb-1 origin-bottom overflow-visible rounded-md border border-neutral-200 dark:border-transparent bg-white dark:bg-neutral-800 shadow-xl z-50 animate-dropdown-in"
                >
                    <div className="p-2 space-y-1">
                        <div className="px-2 py-1 pb-3 border-b border-neutral-100 dark:border-transparent mb-2">
                            <div className="w-5 h-5 rounded bg-gray-200 dark:bg-white/10 flex items-center justify-center flex-shrink-0">
                                <span className="text-xs text-gray-800 dark:text-gray-200 font-medium uppercase">{user?.username?.charAt(0) || 'U'}</span>
                            </div>
                            <div className="text-xs mt-2 font-medium text-neutral-900 dark:text-neutral-100 truncate">{user?.username || 'Account'}</div>
                            {maskedEmail && <div className="text-[10px] text-neutral-500 dark:text-neutral-400 tracking-widest uppercase truncate">{maskedEmail}</div>}
                        </div>
                        
                        <Link 
                            to="/account" 
                            onClick={handleClose}
                            style={{ fontSize: '11px' }}
                            className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-xs text-neutral-800 dark:text-neutral-200 hover:bg-neutral-50 dark:hover:bg-neutral-700 cursor-pointer transition-colors"
                        >
                            <UserIcon className="h-4 w-4"/>
                            <span>Account settings</span>
                        </Link>
                        
                        <div className="relative">
                            <button
                                ref={tenantsTriggerRef}
                                onClick={handleTenantsClick}
                                style={{ fontSize: '11px' }}
                                className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-xs text-neutral-800 dark:text-neutral-200 hover:bg-neutral-50 dark:hover:bg-neutral-700 cursor-pointer transition-colors"
                            >
                                <ArrowsRightLeftIcon className="h-4 w-4"/>
                                <span>Switch tenant</span>
                                <ChevronRightIcon className="h-3 w-3 ml-auto"/>
                            </button>
                            
                            <button 
                                onClick={() => { logout(); handleClose() }}
                                    className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-xs text-neutral-800 dark:text-neutral-200 hover:bg-neutral-50 dark:hover:bg-neutral-700 cursor-pointer transition-colors"
                                    style={{ fontSize: '11px' }}
                            >
                                <ArrowRightOnRectangleIcon className="h-4 w-4"/>
                                Logout
                            </button>
                        </div>
                    </div>
                </div>
            )}
            
            {/* Tenants dropdown rendered as portal to avoid clipping */}
            {tenantsOpen && typeof document !== 'undefined' && createPortal(
                <div 
                    ref={tenantsRef}
                    className="fixed w-64 rounded-md border border-neutral-200 dark:border-transparent bg-white dark:bg-neutral-800 shadow-xl z-[85] animate-dropdown-in"
                    style={{ 
                        left: tenantsPosition.left, 
                        bottom: tenantsPosition.bottom 
                    }}
                >
                    <TenantHub 
                        onBack={() => setTenantsOpen(false)} 
                        onClose={handleClose}
                        onOpenCreate={() => setCreateOpen(true)}
                        onOpenMembers={(tenantId, tenantName) => setMembersState({ open: true, tenantId, tenantName })}
                        onOpenDelete={(tenantId, tenantName) => setDeleteState({ open: true, tenantId, tenantName })}
                    />
                </div>,
                document.body
            )}

            {/* Modals rendered outside dropdown to avoid interference */}
            <TenantCreateModal open={createOpen} onClose={() => setCreateOpen(false)} />
            <TenantMembersModal open={membersState.open} onClose={() => setMembersState({ open: false, tenantId: null, tenantName: '' })} tenantId={membersState.tenantId || ''} tenantName={membersState.tenantName} />
            <TenantDeleteConfirmModal 
                open={deleteState.open} 
                onClose={() => setDeleteState({ open: false, tenantId: null, tenantName: '' })} 
                tenantId={deleteState.tenantId || ''} 
                tenantName={deleteState.tenantName} 
            />
        </div>
    )
}


function TenantHub({ onBack, onClose, onOpenCreate, onOpenMembers, onOpenDelete }: { 
    onBack: () => void; 
    onClose?: () => void; 
    onOpenCreate?: () => void;
    onOpenMembers?: (tenantId: string, tenantName: string) => void;
    onOpenDelete?: (tenantId: string, tenantName: string) => void;
}) {
    const { tenants, selectedTenantId, setSelectedTenantId, refreshTenants } = useTenants()
    const { token, user } = useAuth()
    const getTokenFn = React.useCallback(() => token, [token])
    const api = useApi(getTokenFn)
    const { notify } = useAlert()
    const listRef = React.useRef<HTMLUListElement | null>(null)

    const handleSelect = (tenantId: string) => {
        setSelectedTenantId(tenantId)
        notify({ type: 'success', title: 'Switched tenant', description: tenants.find(t => t.id === tenantId)?.name || '' })
        onClose?.()
    }

    const handleDelete = async (tenantId: string, tenantName: string) => {
        onOpenDelete?.(tenantId, tenantName)
    }

    return (
        <div className="p-4 space-y-2">
            <div className="flex items-center gap-1">
                <span className="text-xs font-medium text-neutral-800 dark:text-neutral-200">Tenants</span>
                <div className="ml-auto">
                    <button style={{ fontSize: '11px' }} onClick={() => onOpenCreate?.()} className="inline-flex items-center gap-1 rounded-full bg-white dark:bg-neutral-700 px-1.5 py-1 text-[11px] text-neutral-700 dark:text-neutral-200 hover:bg-neutral-50 dark:hover:bg-neutral-600 cursor-pointer transition-colors" type="button">
                        <PlusIcon className="h-3 w-3"/> New tenant
                    </button>
                </div>
            </div>
            <div className="max-h-48 overflow-auto rounded border border-neutral-100 dark:border-transparent">
                {tenants.length === 0 ? (
                    <div className="p-2 text-center text-xs text-neutral-500 dark:text-neutral-400">No tenants yet</div>
                ) : (
                        <ul ref={listRef} className="divide-y divide-neutral-100 dark:divide-neutral-700">
                        {tenants.map((t) => (
                            <TenantListRow
                                key={t.id}
                                tenant={t}
                                isCurrent={selectedTenantId === t.id}
                                onSelect={() => handleSelect(t.id)}
                                onMembers={() => onOpenMembers?.(t.id, t.name)}
                                onDelete={() => onOpenDelete?.(t.id, t.name)}
                            />
                        ))}
                    </ul>
                )}
            </div>
        </div>
    )
}


function TenantListRow({ tenant, isCurrent, onSelect, onMembers, onDelete }: { tenant: { id: string; name: string; role?: string }; isCurrent: boolean; onSelect: () => void; onMembers: () => void; onDelete: () => void }) {
    const [menuOpen, setMenuOpen] = React.useState(false)
    const btnRef = React.useRef<HTMLButtonElement | null>(null)
    const isOwner = tenant.role === 'owner'
    return (
        <li className={cn('px-2 py-1.5 text-xs flex items-center justify-between', isCurrent && 'bg-neutral-50 dark:bg-neutral-700')}>
            <button onClick={onSelect} className="truncate text-left w-full text-left cursor-pointer hover:bg-neutral-50 dark:hover:bg-neutral-600 rounded px-1 py-0.5 transition-colors">
                {tenant.name} {isOwner ? <span className="ml-1 rounded bg-yellow-50 text-yellow-700 border border-yellow-200 px-1 py-0 text-[10px]">Owner</span> : null}
            </button>
            {isOwner ? (
                <div className="shrink-0 inline-flex items-center gap-1">
                    <button ref={btnRef} type="button" onClick={(e) => { e.stopPropagation(); setMenuOpen((v) => !v) }} className="inline-flex h-5 w-5 items-center justify-center rounded hover:bg-neutral-100 dark:hover:bg-neutral-600 cursor-pointer transition-colors">
                        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" className="h-4 w-4 text-neutral-500 dark:text-neutral-400"><path fillRule="evenodd" d="M12 6.75a1.5 1.5 0 110-3 1.5 1.5 0 010 3zm0 6a1.5 1.5 0 110-3 1.5 1.5 0 010 3zm-1.5 7.5a1.5 1.5 0 103 0 1.5 1.5 0 00-3 0z" clipRule="evenodd"/></svg>
                    </button>
                    {menuOpen ? (
                        <MenuPortal anchorRef={btnRef} onClose={() => setMenuOpen(false)}>
                            <div className="p-1 text-xs">
                                <button 
                                    onClick={(e) => { 
                                        e.preventDefault(); 
                                        e.stopPropagation(); 
                                        setMenuOpen(false);
                                        setTimeout(() => onMembers(), 10);
                                    }} 
                                    className="flex w-full items-center rounded px-2 py-1.5 hover:bg-neutral-50 dark:hover:bg-neutral-700 cursor-pointer transition-colors"
                                >
                                    <span>Members</span>
                                </button>
                                <button 
                                    onClick={(e) => { 
                                        e.preventDefault(); 
                                        e.stopPropagation(); 
                                        setMenuOpen(false);
                                        setTimeout(() => onDelete(), 10);
                                    }} 
                                    className="flex w-full items-center rounded px-2 py-1.5 hover:bg-neutral-50 dark:hover:bg-neutral-700 text-red-600 dark:text-red-400 cursor-pointer transition-colors"
                                >
                                    <span>Delete tenant</span>
                                </button>
                            </div>
                        </MenuPortal>
                    ) : null}
                </div>
            ) : null}
        </li>
    )
}



function TenantCreateModal({ open, onClose }: { open: boolean; onClose: () => void }) {
    const { token } = useAuth()
    const getTokenFn = React.useCallback(() => token, [token])
    const api = useApi(getTokenFn)
    const { refreshTenants } = useTenants()
    const { notify } = useAlert()
    const [name, setName] = React.useState('')
    const [isLoading, setIsLoading] = React.useState(false)

    const handleCreate = async (e: React.FormEvent) => {
        e.preventDefault()
        if (!name) return
        setIsLoading(true)
        try {
            await api.tenants.create({ name })
            await refreshTenants()
            notify({ type: 'success', title: 'Tenant created', description: name })
            setName('')
            onClose()
        } catch (e: any) {
            notify({ type: 'error', title: 'Failed to create tenant', description: e?.message || '' })
        } finally {
            setIsLoading(false)
        }
    }

    return (
        <Modal open={open} onClose={onClose} title="Create tenant" subtitle="Organization" zIndex={90}>
            <form onSubmit={handleCreate} className="space-y-3">
                <div>
                    <Label htmlFor="tenant-name">Tenant name</Label>
                    <Input id="tenant-name" value={name} onChange={(e) => setName(e.target.value)} />
                </div>
                <div className="flex justify-end gap-2 pt-1">
                    <Button type="button" variant="ghost" size="sm" onClick={onClose}>Cancel</Button>
                    <Button type="submit" variant="primary" size="sm" isLoading={isLoading}>Create</Button>
                </div>
            </form>
        </Modal>
    )
}

function TenantDeleteConfirmModal({ open, onClose, tenantId, tenantName }: { open: boolean; onClose: () => void; tenantId: string; tenantName: string }) {
    const { token } = useAuth()
    const getTokenFn = React.useCallback(() => token, [token])
    const api = useApi(getTokenFn)
    const { refreshTenants } = useTenants()
    const { notify } = useAlert()
    const [isLoading, setIsLoading] = React.useState(false)

    const handleDelete = async () => {
        setIsLoading(true)
        try {
            await api.tenants.delete(tenantId)
            await refreshTenants()
            notify({ type: 'success', title: 'Deleted tenant', description: tenantName })
            onClose()
        } catch (e: any) {
            notify({ type: 'error', title: 'Failed to delete tenant', description: e?.message || '' })
        } finally {
            setIsLoading(false)
        }
    }

    return (
        <Modal open={open} onClose={onClose} title="Delete tenant" subtitle="This action is destructive">
            <div className="space-y-3 text-xs">
                <p>Are you sure you want to delete <span className="font-semibold">{tenantName}</span>? This cannot be undone.</p>
                <div className="flex justify-end gap-2 pt-1">
                    <Button type="button" variant="ghost" size="sm" onClick={onClose}>Cancel</Button>
                    <Button type="button" variant="destructive" size="sm" isLoading={isLoading} onClick={handleDelete}>Delete</Button>
                </div>
            </div>
        </Modal>
    )
}

function TenantMembersModal({ open, onClose, tenantId, tenantName }: { open: boolean; onClose: () => void; tenantId: string; tenantName: string }) {
    const { token } = useAuth()
    const getTokenFn = React.useCallback(() => token, [token])
    const api = useApi(getTokenFn)
    const { notify } = useAlert()
    const { user } = useAuth()
    const [members, setMembers] = React.useState<Array<{ userId: string; role: string; email: string; username?: string }>>([])
    const [inviteOpen, setInviteOpen] = React.useState(false)

    React.useEffect(() => {
        if (!open) return
        let ignore = false
        ;(async () => {
            try {
                const r = await api.tenants.members(tenantId)
                if (!ignore) setMembers((r.items || []) as any)
            } catch {}
        })()
        return () => { ignore = true }
    }, [open, tenantId])

    const handleRemove = async (targetUserId: string) => {
        try {
            await api.tenants.removeMember(tenantId, targetUserId)
            const r = await api.tenants.members(tenantId)
            setMembers((r.items || []) as any)
            notify({ type: 'success', title: 'Member removed' })
        } catch (e: any) {
            notify({ type: 'error', title: 'Failed to remove', description: e?.message || '' })
        }
    }

    return (
        <Modal open={open} onClose={onClose} title={`Members — ${tenantName}`} subtitle="Manage access">
            <div className="space-y-3">
                <div className="flex justify-between items-center">
                    <div className="text-[11px] text-gray-500 tracking-widest uppercase">Members</div>
                    <Button size="sm" onClick={() => setInviteOpen(true)}>Invite</Button>
                </div>
                <div className="max-h-72 overflow-auto rounded border border-gray-100 dark:border-neutral-700/80">
                    <table className="w-full text-xs text-neutral-800 dark:text-neutral-100">
                        <thead className="bg-gray-50 text-gray-600 dark:bg-neutral-800/80 dark:text-neutral-300">
                            <tr>
                                <th className="text-left px-2 py-1.5 font-medium">Name</th>
                                <th className="text-left px-2 py-1.5 font-medium">Email</th>
                                <th className="text-left px-2 py-1.5 font-medium">Role</th>
                                <th className="text-right px-2 py-1.5 font-medium">Actions</th>
                            </tr>
                        </thead>
                        <tbody className="bg-white dark:bg-neutral-900/70">
                            {members.length === 0 ? (
                                <tr><td colSpan={4} className="px-2 py-3 text-center text-gray-500 dark:text-neutral-400">No members</td></tr>
                            ) : members.map((m) => {
                                const isOwner = m.role === 'owner'
                                const isSelf = !!(user?.id && m.userId === user.id)
                                return (
                                    <tr key={m.userId} className="border-t border-gray-100 dark:border-neutral-800">
                                        <td className="px-2 py-1.5 truncate">{m.username || '-'}</td>
                                        <td className="px-2 py-1.5 truncate">{m.email}</td>
                                        <td className="px-2 py-1.5">{m.role}</td>
                                        <td className="px-2 py-1.5 text-right">
                                            <button
                                                onClick={() => handleRemove(m.userId)}
                                                className={cn('hover:underline transition-colors', (isOwner || isSelf) ? 'text-gray-400 cursor-not-allowed' : 'text-red-600 cursor-pointer')}
                                                disabled={isOwner || isSelf}
                                                title={isOwner ? 'Cannot remove owner' : (isSelf ? 'Cannot remove yourself' : 'Remove member')}
                                            >
                                                Remove
                                            </button>
                                        </td>
                                    </tr>
                                )
                            })}
                        </tbody>
                    </table>
                </div>
                <div className="flex justify-end">
                    <Button variant="ghost" size="sm" onClick={onClose}>Close</Button>
                </div>
            </div>
            <TenantInviteModal
                open={inviteOpen}
                onClose={() => setInviteOpen(false)}
                zIndex={90}
                onInvited={async (email) => {
                    try {
                        await api.tenants.addMemberByEmail(tenantId, email)
                        const r2 = await api.tenants.members(tenantId)
                        setMembers(r2.items || [])
                        notify({ type: 'success', title: 'Member invited', description: email })
                    } catch (e: any) {
                        notify({ type: 'error', title: 'Failed to invite', description: e?.message || '' })
                    }
                }}
            />
        </Modal>
    )
}

function TenantInviteModal({ open, onClose, onInvited, zIndex }: { open: boolean; onClose: () => void; onInvited: (email: string) => Promise<void>; zIndex?: number }) {
    const [email, setEmail] = React.useState('')
    const [isLoading, setIsLoading] = React.useState(false)
    const handleSubmit = async (e: React.FormEvent) => {
        e.preventDefault()
        if (!email) return
        setIsLoading(true)
        try {
            await onInvited(email)
            setEmail('')
            onClose()
        } finally {
            setIsLoading(false)
        }
    }
    return (
        <Modal open={open} onClose={onClose} title="Invite member" subtitle="Enter email" zIndex={zIndex}
        >
            <form onSubmit={handleSubmit} className="space-y-3">
                <div>
                    <Label htmlFor="tenant-invite-email">Email</Label>
                    <Input id="tenant-invite-email" type="email" value={email} onChange={(e) => setEmail(e.target.value)} />
                </div>
                <div className="flex justify-end gap-2 pt-1">
                    <Button type="button" variant="ghost" size="sm" onClick={onClose}>Cancel</Button>
                    <Button type="submit" size="sm" isLoading={isLoading}>Invite</Button>
                </div>
            </form>
        </Modal>
    )
}


function MenuPortal({ anchorRef, onClose, children }: { anchorRef: React.RefObject<HTMLElement | null>; onClose: () => void; children: React.ReactNode }) {
    const [pos, setPos] = React.useState<{ left: number; top: number }>({ left: 0, top: 0 })
    React.useLayoutEffect(() => {
        const el = anchorRef.current
        if (!el) return
        const rect = el.getBoundingClientRect()
        setPos({ left: rect.right - 160, top: rect.bottom + 6 })
    }, [anchorRef])
    React.useEffect(() => {
        function handle(e: MouseEvent) {
            const target = e.target as HTMLElement | null
            // If click is inside the menu or the anchor button, don't close
            const menu = document.getElementById('tenant-menu-popover')
            if (menu && menu.contains(target)) return
            const anchor = anchorRef.current
            if (anchor && anchor.contains(target)) return
            
            // Don't close if clicking on modal elements - be more permissive
            const modalElement = target?.closest('[role="dialog"], .fixed')
            if (modalElement) return
            
            // Only close the menu, don't prevent or stop anything
            onClose()
        }
        document.addEventListener('mousedown', handle)
        return () => document.removeEventListener('mousedown', handle)
    }, [onClose, anchorRef])
    if (typeof document === 'undefined') return null
    return createPortal(
        <div id="tenant-menu-popover" style={{ position: 'fixed', left: pos.left, top: pos.top, zIndex: 87 }} className="min-w-[160px] overflow-hidden rounded-md border border-neutral-200 dark:border-transparent bg-white dark:bg-neutral-800 shadow-xl">
            {children}
        </div>,
        document.body
    )
}

export default Sidebar