import React, { createContext, useCallback, useContext, useMemo, useRef, useState } from 'react'
import { cn } from '@/utils/cn'

type AlertType = 'success' | 'error' | 'warning' | 'info'

export interface AlertOptions {
    title?: string
    description?: string
    type?: AlertType
    durationMs?: number
}

interface AlertItem extends Required<AlertOptions> { id: string }

interface AlertContextValue {
    notify: (options: AlertOptions) => void
}

const AlertContext = createContext<AlertContextValue | undefined>(undefined)

const typeToColor: Record<AlertType, string> = {
    success: '#16a34a',
    error: '#dc2626',
    warning: '#dc2626',
    info: '#16a34a',
}

const typeToIcon: Record<AlertType, React.ReactNode> = {
    success: (
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" width="20" height="20">
            <path fillRule="evenodd" d="M2.25 12c0-5.385 4.365-9.75 9.75-9.75s9.75 4.365 9.75 9.75-4.365 9.75-9.75 9.75S2.25 17.385 2.25 12Zm13.36-2.59a.75.75 0 10-1.22-.86l-3.44 4.88-1.79-1.79a.75.75 0 10-1.06 1.06l2.4 2.4a.75.75 0 001.16-.09l3.95-5.6Z" clipRule="evenodd" />
        </svg>
    ),
    error: (
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" width="20" height="20">
            <path fillRule="evenodd" d="M12 2.25c-5.385 0-9.75 4.365-9.75 9.75s4.365 9.75 9.75 9.75 9.75-4.365 9.75-9.75S17.385 2.25 12 2.25Zm-.75 5.25a.75.75 0 011.5 0v6a.75.75 0 01-1.5 0v-6Zm.75 10.5a1.125 1.125 0 100-2.25 1.125 1.125 0 000 2.25Z" clipRule="evenodd" />
        </svg>
    ),
    warning: (
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" width="20" height="20">
            <path fillRule="evenodd" d="M10.788 3.21c.448-.772 1.58-.772 2.028 0l8.602 14.833c.447.771-.112 1.957-1.014 1.957H3.2c-.902 0-1.46-1.186-1.014-1.957L10.788 3.21Zm1.212 5.79a.75.75 0 00-1.5 0v4.5a.75.75 0 001.5 0V9Zm-.75 9a1.125 1.125 0 100-2.25 1.125 1.125 0 000 2.25Z" clipRule="evenodd" />
        </svg>
    ),
    info: (
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" width="20" height="20">
            <path fillRule="evenodd" d="M12 2.25c-5.385 0-9.75 4.365-9.75 9.75s4.365 9.75 9.75 9.75 9.75-4.365 9.75-9.75S17.385 2.25 12 2.25Zm-.75 6a.75.75 0 011.5 0v.75a.75.75 0 01-1.5 0V8.25Zm0 3a.75.75 0 011.5 0v6a.75.75 0 01-1.5 0v-6Z" clipRule="evenodd" />
        </svg>
    ),
}

export const AlertProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
    const [items, setItems] = useState<AlertItem[]>([])
    const idRef = useRef(0)
    const [leavingIds, setLeavingIds] = useState<Set<string>>(new Set())

    const requestClose = useCallback((id: string, delayMs = 220) => {
        setLeavingIds((prev) => new Set(prev).add(id))
        window.setTimeout(() => {
            setItems((prev) => prev.filter((x) => x.id !== id))
            setLeavingIds((prev) => {
                const next = new Set(prev)
                next.delete(id)
                return next
            })
        }, delayMs)
    }, [])

    const notify = useCallback((options: AlertOptions) => {
        const id = String(++idRef.current)
        const item: AlertItem = {
            id,
            title: options.title || (options.type === 'success' ? 'Success' : options.type === 'error' ? 'Error' : options.type === 'warning' ? 'Warning' : 'Notice'),
            description: options.description || '',
            type: options.type || 'info',
            durationMs: options.durationMs ?? 4000,
        }
        setItems((prev) => [item, ...prev].slice(0, 6))
        if (item.durationMs > 0) setTimeout(() => requestClose(id), item.durationMs)
    }, [])

    const value = useMemo<AlertContextValue>(() => ({ notify }), [notify])

    return (
        <AlertContext.Provider value={value}>
            {children}
            <div className="fixed left-1/2 top-4 -translate-x-1/2 z-[100] pointer-events-none" style={{ perspective: 1000 }}>
                {(() => {
                    const depth = Math.min(items.length, 4)
                    const containerHeight = 56 + (depth - 1) * 8
                    return (
                        <div className="relative w-[960px] max-w-[98vw]" style={{ height: containerHeight }}>
                            {items.map((a, index) => {
                                const isLeaving = leavingIds.has(a.id)
                                const cappedIndex = Math.min(index, 3)
                                const topOffset = cappedIndex * 8
                                const scale = 1 - cappedIndex * 0.025
                                const contentBlurCls = index === 0 ? 'blur-0' : index === 1 ? 'blur-[1px]' : 'blur-[2px]'
                                const stackOpacity = Math.max(0.7, 1 - index * 0.12)
                                const bgClass = a.type === 'success' || a.type === 'info' ? 'bg-emerald-700/90' : 'bg-red-700/90'
                                return (
                                    <div
                                        key={a.id}
                                        className={cn(
                                            'absolute left-1/2 -translate-x-1/2 will-change-transform backdrop-blur rounded-lg',
                                            isLeaving
                                                ? 'animate-[alert-out_200ms_cubic-bezier(0.22,1,0.36,1)_forwards]'
                                                : 'animate-[alert-in_260ms_cubic-bezier(0.16,1,0.3,1)_both]'
                                        )}
                                        style={{ top: topOffset, zIndex: items.length - index }}
                                    >
                                        <div
                                            className={cn(
                                                'relative overflow-hidden rounded-lg ring-1 ring-white/15 shadow-2xl backdrop-blur-2xl backdrop-saturate-150',
                                                'px-5 h-14 flex items-center gap-3 text-white',
                                                bgClass,
                                                index > 0 ? 'pointer-events-none' : 'pointer-events-auto'
                                            )}
                                            style={{ transform: `scale(${scale})`, opacity: stackOpacity, WebkitBackdropFilter: 'blur(20px) saturate(140%)', backdropFilter: 'blur(20px) saturate(140%)' }}
                                            role="status"
                                            aria-live="polite"
                                        >
                                            <div className="pointer-events-none absolute inset-0 bg-white/5 [mask-image:radial-gradient(120%_60%_at_50%_-20%,black,transparent)]" />
                                            <div className={cn('relative z-10 flex w-full items-center gap-3', contentBlurCls)}>
                                                <div className="flex items-center justify-center text-white/95">
                                                    {typeToIcon[a.type]}
                                                </div>
                                                <div className="min-w-0 flex-1">
                                                    <div className="text-sm font-semibold leading-tight truncate">{a.title}</div>
                                                    {a.description ? (
                                                        <div className="text-xs/5 text-white/90 truncate">{a.description}</div>
                                                    ) : null}
                                                </div>
                                                <button
                                                    type="button"
                                                    aria-label="Close alert"
                                                    className="shrink-0 ml-2 inline-flex h-6 w-6 items-center justify-center rounded-md text-white/90 hover:text-white hover:bg-white/10 active:bg-white/15 transition"
                                                    onClick={() => requestClose(a.id)}
                                                >
                                                    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" width="18" height="18" aria-hidden="true">
                                                        <path fillRule="evenodd" d="M5.47 5.47a.75.75 0 011.06 0L12 10.94l5.47-5.47a.75.75 0 111.06 1.06L13.06 12l5.47 5.47a.75.75 0 11-1.06 1.06L12 13.06l-5.47 5.47a.75.75 0 11-1.06-1.06L10.94 12 5.47 6.53a.75.75 0 010-1.06z" clipRule="evenodd" />
                                                    </svg>
                                                </button>
                                            </div>
                                        </div>
                                    </div>
                                )
                            })}
                        </div>
                    )
                })()}
            </div>
        </AlertContext.Provider>
    )
}

export function useAlert() {
    const ctx = useContext(AlertContext)
    if (!ctx) throw new Error('useAlert must be used within AlertProvider')
    return ctx
}


