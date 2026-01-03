import React, { useCallback, useEffect, useMemo, useState } from 'react'
import { useAuth } from '@/providers/AuthProvider'
import { useApi } from '@/api/client'
import Spinner from '@/components/ui/Spinner'
import { Button } from '@/components/ui/Button'
import { Select } from '@/components/ui/Select'
import LoadingAnimation from '@/components/loaders'

interface CreditTransaction {
    id: string
    amount: number
    balanceAfter: number
    type: string
    description: string
    referenceId?: string
    referenceType?: string
    createdAt: string
    createdBy?: string
}

// Resource bar with gradient (same as Dashboard)
function ResourceBar({ label, used, total, unit }: { label: string; used: number; total: number; unit: string }) {
    const percent = total > 0 ? Math.round((used / total) * 100) : 0
    const barSegments = 20
    const filledSegments = Math.round((percent / 100) * barSegments)
    
    return (
        <div className="space-y-2">
            <div className="flex items-center justify-between">
                <span className="text-[10px] uppercase tracking-[0.2em] text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                    {label}
                </span>
                <span className="text-xs text-neutral-700 dark:text-neutral-400" style={{ fontFamily: "'Space Mono', monospace" }}>
                    {used.toFixed(2)}{unit} / {total.toFixed(2)}{unit}
                </span>
            </div>
            <div className="flex gap-[2px]">
                {Array.from({ length: barSegments }).map((_, i) => {
                    const isFilled = i < filledSegments
                    // Gradient from yellow to orange to red
                    const segmentPercent = (i / barSegments) * 100
                    let color = 'bg-neutral-300 dark:bg-neutral-800'
                    if (isFilled) {
                        if (segmentPercent < 33) color = 'bg-yellow-500'
                        else if (segmentPercent < 66) color = 'bg-orange-500'
                        else color = 'bg-red-500'
                    }
                    return (
                        <div
                            key={i}
                            className={`h-4 flex-1 ${color} ${isFilled ? 'opacity-100' : 'opacity-30'}`}
                        />
                    )
                })}
            </div>
            <div className="text-right">
                <span className="text-lg text-red-600 dark:text-red-400" style={{ fontFamily: "'Seven Segment', sans-serif" }}>
                    {percent}%
                </span>
            </div>
        </div>
    )
}

export function BillingPage() {
    const { token } = useAuth()
    const getTokenFn = useCallback(() => token, [token])
    const api = useApi(getTokenFn)
    
    const [ruBalance, setRuBalance] = useState<number>(0)
    const [usedResourceUnits, setUsedResourceUnits] = useState<number>(0)
    const [transactions, setTransactions] = useState<CreditTransaction[]>([])
    const [transactionTotal, setTransactionTotal] = useState(0)
    const [transactionPage, setTransactionPage] = useState(1)
    const [transactionPageSize, setTransactionPageSize] = useState(10)
    const [loading, setLoading] = useState(true)
    const [transactionLoading, setTransactionLoading] = useState(false)
    const [error, setError] = useState<string | null>(null)

    const totalPages = Math.max(1, Math.ceil(transactionTotal / transactionPageSize) || 1)
    const hasPagination = transactionTotal > transactionPageSize

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

    const loadTransactions = useCallback(async (page: number, abortRef?: { current: boolean }) => {
        setTransactionLoading(true)
        try {
            const response = await api.wallet.getTransactions(page, transactionPageSize)
            if (abortRef?.current) return
            setTransactions(response.items || [])
            setTransactionTotal(response.meta?.total || 0)
            setTransactionPage(page)
        } catch (e) {
            if (!abortRef?.current) {
                console.error('Failed to load transactions:', e)
            }
        } finally {
            if (!abortRef?.current) setTransactionLoading(false)
        }
    }, [api, transactionPageSize])

    const handleTransactionPageChange = useCallback((nextPage: number) => {
        if (nextPage < 1 || nextPage === transactionPage) return
        if (nextPage > totalPages) return
        loadTransactions(nextPage)
    }, [transactionPage, loadTransactions, totalPages])

    const handleTransactionPageSizeChange = useCallback((value: string) => {
        const size = parseInt(value, 10)
        if (!Number.isFinite(size) || size <= 0) return
        setTransactionPageSize(size)
        setTransactionPage(1)
        loadTransactions(1)
    }, [loadTransactions])

    const loadData = useCallback(async () => {
        setLoading(true)
        setError(null)
        const abortRef = { current: false }
        
        try {
            const walletRes = await api.wallet.get()
            setRuBalance(walletRes.ruBalance || 0)
            setUsedResourceUnits(walletRes.usedResourceUnits || 0)
            await loadTransactions(1, abortRef)
        } catch (e: any) {
            if (!abortRef.current) {
                setError(e?.message || 'Failed to load billing data')
            }
        } finally {
            if (!abortRef.current) setLoading(false)
        }
        
        return () => { abortRef.current = true }
    }, [api, loadTransactions])

    useEffect(() => {
        loadData()
    }, [loadData])

    const displayedTransactions = useMemo(() => transactions, [transactions])

    if (loading) return (
        <div className="flex items-center justify-center py-12"><LoadingAnimation/></div>
    )

    if (error) return (
        <div className="bg-neutral-100 dark:bg-black border border-neutral-300 dark:border-neutral-800/50 p-6">
            <div className="text-red-600 dark:text-red-400" style={{ fontFamily: "'Space Mono', monospace" }}>{error}</div>
        </div>
    )

    return (
        <div className="space-y-8">
            {/* Wallet Overview Section */}
            <div className="bg-neutral-100 dark:bg-black border border-neutral-300 dark:border-neutral-800/50 p-6">
                <div className="flex items-center gap-3 mb-6">
                    <h2 
                        className="text-2xl text-neutral-900 dark:text-neutral-100 tracking-wider"
                        style={{ fontFamily: "'Seven Segment', sans-serif" }}
                    >
                        WALLET OVERVIEW
                    </h2>
                    <div className="flex-1 h-px bg-neutral-300 dark:bg-neutral-800"></div>
                </div>

                <div className="space-y-6">
                    {loading ? (
                        Array.from({ length: 2 }).map((_, i) => (
                            <div key={i} className="space-y-2 animate-pulse">
                                <div className="flex items-center justify-between">
                                    <div className="h-3 w-20 bg-neutral-200 dark:bg-neutral-800 rounded"></div>
                                    <div className="h-3 w-24 bg-neutral-200 dark:bg-neutral-800 rounded"></div>
                                </div>
                                <div className="flex gap-[2px]">
                                    {Array.from({ length: 20 }).map((_, j) => (
                                        <div key={j} className="h-4 flex-1 bg-neutral-200 dark:bg-neutral-800/50" />
                                    ))}
                                </div>
                                <div className="flex justify-end">
                                    <div className="h-6 w-12 bg-neutral-200 dark:bg-neutral-800 rounded"></div>
                                </div>
                            </div>
                        ))
                    ) : (
                        <>
                            <ResourceBar 
                                label="RU Balance" 
                                used={ruBalance} 
                                total={Math.max(ruBalance + usedResourceUnits, 100)} 
                                unit=" RU" 
                            />
                            <ResourceBar 
                                label="Total Used (Lifetime)" 
                                used={usedResourceUnits} 
                                total={Math.max(ruBalance + usedResourceUnits, 100)} 
                                unit=" RU" 
                            />
                        </>
                    )}
                </div>
            </div>

            {/* Transaction History */}
            <div className="bg-neutral-100 dark:bg-black border border-neutral-300 dark:border-neutral-800/50 p-6">
                <div className="flex items-center justify-between gap-4 mb-6">
                    <div className="flex items-center gap-3">
                        <h2 
                            className="text-2xl text-neutral-900 dark:text-neutral-100 tracking-wider"
                            style={{ fontFamily: "'Seven Segment', sans-serif" }}
                        >
                            TRANSACTION HISTORY
                        </h2>
                        <div className="flex-1 h-px bg-neutral-300 dark:bg-neutral-800"></div>
                    </div>
                    <div className="flex items-center gap-3 text-xs text-neutral-600 dark:text-neutral-500">
                        <div className="flex items-center gap-2">
                            <span className="whitespace-nowrap" style={{ fontFamily: "'Space Mono', monospace" }}>Rows</span>
                            <div className="w-20">
                                <Select
                                    value={String(transactionPageSize)}
                                    onChange={handleTransactionPageSizeChange}
                                    options={[
                                        { value: '3', label: '3' },
                                        { value: '10', label: '10' },
                                        { value: '25', label: '25' },
                                        { value: '50', label: '50' },
                                    ]}
                                />
                            </div>
                        </div>
                        {hasPagination && (
                            <div className="flex items-center gap-2" style={{ fontFamily: "'Space Mono', monospace" }}>
                                <Button
                                    variant="ghost"
                                    size="sm"
                                    className="h-7 w-7 p-0 text-neutral-600 dark:text-neutral-500 hover:text-neutral-900 dark:hover:text-neutral-300"
                                    onClick={() => handleTransactionPageChange(transactionPage - 1)}
                                    disabled={transactionPage === 1 || transactionLoading}
                                >
                                    ‹
                                </Button>
                                <span className="text-neutral-700 dark:text-neutral-400">
                                    {transactionPage}/{totalPages}
                                </span>
                                <Button
                                    variant="ghost"
                                    size="sm"
                                    className="h-7 w-7 p-0 text-neutral-600 dark:text-neutral-500 hover:text-neutral-900 dark:hover:text-neutral-300"
                                    onClick={() => handleTransactionPageChange(transactionPage + 1)}
                                    disabled={transactionPage >= totalPages || transactionLoading}
                                >
                                    ›
                                </Button>
                            </div>
                        )}
                    </div>
                </div>

                <div className="bg-neutral-50 dark:bg-neutral-900/50 border border-neutral-300 dark:border-neutral-800/50 overflow-hidden">
                    {transactionLoading && transactions.length === 0 ? (
                        <div className="p-8 flex items-center justify-center">
                            <Spinner size="lg" />
                        </div>
                    ) : displayedTransactions.length === 0 ? (
                        <div className="p-8 text-center">
                            <p className="text-sm text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                                No transactions yet
                            </p>
                        </div>
                    ) : (
                        <div className="divide-y divide-neutral-200 dark:divide-neutral-800/50">
                            {displayedTransactions.map((tx) => {
                                const isCredit = tx.amount >= 0
                                const typeLabel = tx.type === 'admin_credit' ? 'Credit' : tx.type === 'usage_deduct' ? 'Usage' : tx.type === 'admin_set' ? 'Admin Set' : tx.type
                                
                                return (
                                    <div key={tx.id} className="p-4 hover:bg-neutral-100 dark:hover:bg-neutral-800/20 transition-colors">
                                        <div className="flex items-start justify-between gap-4">
                                            <div className="flex items-start gap-3 flex-1">
                                                <span className={`w-2 h-2 rounded-full mt-1.5 ${isCredit ? 'bg-green-500' : 'bg-red-500'}`}></span>
                                                <div className="flex-1 space-y-1">
                                                    <div className="flex items-center gap-2 flex-wrap">
                                                        <span className="text-sm font-medium text-neutral-900 dark:text-neutral-200" style={{ fontFamily: "'Space Mono', monospace" }}>
                                                            {tx.description}
                                                        </span>
                                                        <span className={`text-xs px-2 py-0.5 rounded ${
                                                            isCredit 
                                                                ? 'text-green-600 dark:text-green-400 bg-green-500/10'
                                                                : 'text-red-600 dark:text-red-400 bg-red-500/10'
                                                        }`} style={{ fontFamily: "'Space Mono', monospace" }}>
                                                            {typeLabel}
                                                        </span>
                                                    </div>
                                                    <div className="flex flex-wrap gap-x-4 gap-y-1 text-[10px] text-neutral-600 dark:text-neutral-500" style={{ fontFamily: "'Space Mono', monospace" }}>
                                                        <span className={isCredit ? 'text-green-600 dark:text-green-400' : 'text-red-600 dark:text-red-400'}>
                                                            {isCredit ? '+' : ''}{tx.amount.toFixed(2)} RU
                                                        </span>
                                                        <span>Balance: {tx.balanceAfter.toFixed(2)} RU</span>
                                                        {tx.referenceId && (
                                                            <span>Ref: {tx.referenceId.slice(0, 8)}...</span>
                                                        )}
                                                        {tx.createdBy && (
                                                            <span>By: {tx.createdBy}</span>
                                                        )}
                                                    </div>
                                                </div>
                                            </div>
                                            <span className="text-[10px] text-neutral-500 dark:text-neutral-600 uppercase tracking-wider whitespace-nowrap" style={{ fontFamily: "'Space Mono', monospace" }}>
                                                {formatTimestamp(tx.createdAt)}
                                            </span>
                                        </div>
                                    </div>
                                )
                            })}
                        </div>
                    )}
                </div>
            </div>
        </div>
    )
}
