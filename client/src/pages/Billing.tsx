import React, { useCallback, useEffect, useState } from 'react'
import { useAuth } from '@/providers/AuthProvider'
import { useApi } from '@/api/client'
import { useTenants } from '@/providers/TenantProvider'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card'
import { Button } from '@/components/ui/Button'
import { Input } from '@/components/ui/Input'
import { Label } from '@/components/ui/Label'
import { Modal } from '@/components/ui/Modal'
import Spinner from '@/components/ui/Spinner'
import { PlusIcon, CurrencyDollarIcon, ClockIcon, CpuChipIcon } from '@heroicons/react/24/outline'
import { useAlert } from '@/components/ui/Alert'

interface BillingConfig {
    enabled: boolean
    currency: string
    free_quota_hours: number
    tracking_interval: number
    price_per_gb_ram: number
    price_per_cpu_core: number
    price_per_gb_disk: number
    price_per_gb_network: number
}

interface ContainerBilling {
    container_id: string
    container_name: string
    tenant_id: string
    start_time: string
    last_update: string
    total_runtime_hours: number
    total_cost: number
    resource_units: number
    free_quota_used_hours: number
    billable_hours: number
}

interface TenantBilling {
    tenant_id: string
    containers: ContainerBilling[]
    total_cost: number
    currency: string
}

interface Transaction {
    id: string
    amount: number
    currency: string
    description: string
    type: string
    status: string
    createdAt: string
    completedAt?: string
}

export function BillingPage() {
    const { token } = useAuth()
    const getTokenFn = useCallback(() => token, [token])
    const api = useApi(getTokenFn)
    const { selectedTenantId } = useTenants()
    const { notify } = useAlert()
    
    const [billing, setBilling] = useState<TenantBilling | null>(null)
    const [balance, setBalance] = useState<{ balance: number; currency: string; lastUpdated: string } | null>(null)
    const [config, setConfig] = useState<BillingConfig | null>(null)
    const [transactions, setTransactions] = useState<Transaction[]>([])
    const [loading, setLoading] = useState(false)
    const [error, setError] = useState<string | null>(null)
    const [showAddFundsModal, setShowAddFundsModal] = useState(false)
    const [addingFunds, setAddingFunds] = useState(false)
    const [fundAmount, setFundAmount] = useState('')

    const loadBillingData = useCallback(async () => {
        if (!selectedTenantId) return
        setLoading(true)
        setError(null)
        
        try {
            const [billingRes, configRes, transactionsRes] = await Promise.all([
                api.billing.getTenantBilling(selectedTenantId),
                api.billing.getConfig(),
                api.billing.getTransactions(selectedTenantId)
            ])
            
            setBilling(billingRes.billing)
            setBalance(billingRes.balance)
            setConfig(configRes)
            setTransactions(transactionsRes.items || [])
        } catch (e: any) {
            setError(e?.message || 'Failed to load billing data')
        } finally {
            setLoading(false)
        }
    }, [api, selectedTenantId])

    useEffect(() => {
        loadBillingData()
    }, [loadBillingData])

    const handleAddFunds = async (e: React.FormEvent) => {
        e.preventDefault()
        if (!selectedTenantId) return
        
        const amount = parseFloat(fundAmount)
        if (isNaN(amount) || amount <= 0) {
            notify({ type: 'error', description: 'Please enter a valid amount' })
            return
        }
        
        setAddingFunds(true)
        try {
            await api.billing.addFunds(selectedTenantId, amount)
            notify({ type: 'success', description: `Added $${amount.toFixed(2)} to your balance` })
            setShowAddFundsModal(false)
            setFundAmount('')
            loadBillingData()
        } catch (e: any) {
            notify({ type: 'error', description: e?.message || 'Failed to add funds' })
        } finally {
            setAddingFunds(false)
        }
    }

    if (!selectedTenantId) return (
        <Card><CardHeader><CardTitle>Select a tenant</CardTitle></CardHeader></Card>
    )

    if (loading) return (
        <div className="flex items-center justify-center py-12"><Spinner size="lg" /></div>
    )

    if (error) return (
        <Card><CardHeader><CardTitle className="text-red-600">{error}</CardTitle></CardHeader></Card>
    )

    const totalCost = billing?.total_cost || 0
    const currentBalance = balance?.balance || 0
    const currency = config?.currency || 'USD'

    return (
        <div className="space-y-6">
            <div className="flex items-center justify-between">
                <div>
                    <h1 className="text-2xl font-semibold text-neutral-900 dark:text-neutral-100">Billing & Usage</h1>
                    <p className="text-sm text-neutral-600 dark:text-neutral-400">Track your resource usage and costs</p>
                </div>
                <Button onClick={() => setShowAddFundsModal(true)} className="flex items-center gap-2 shrink-0">
                    <PlusIcon className="w-4 h-4" />
                    Add Funds
                </Button>
            </div>

            {/* Balance & Cost Overview */}
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                <Card>
                    <CardHeader>
                        <CardTitle className="text-sm font-medium text-neutral-600 dark:text-neutral-400">
                            Current Balance
                        </CardTitle>
                    </CardHeader>
                    <CardContent>
                        <div className="flex items-center gap-2">
                            <CurrencyDollarIcon className="w-8 h-8 text-green-600 dark:text-green-400" />
                            <div>
                                <div className="text-2xl font-bold text-neutral-900 dark:text-neutral-100">
                                    ${currentBalance.toFixed(2)}
                                </div>
                                <div className="text-xs text-neutral-500 dark:text-neutral-400">
                                    {currency}
                                </div>
                            </div>
                        </div>
                    </CardContent>
                </Card>

                <Card>
                    <CardHeader>
                        <CardTitle className="text-sm font-medium text-neutral-600 dark:text-neutral-400">
                            Total Usage Cost
                        </CardTitle>
                    </CardHeader>
                    <CardContent>
                        <div className="flex items-center gap-2">
                            <ClockIcon className="w-8 h-8 text-blue-600 dark:text-blue-400" />
                            <div>
                                <div className="text-2xl font-bold text-neutral-900 dark:text-neutral-100">
                                    ${totalCost.toFixed(2)}
                                </div>
                                <div className="text-xs text-neutral-500 dark:text-neutral-400">
                                    All containers
                                </div>
                            </div>
                        </div>
                    </CardContent>
                </Card>

                <Card>
                    <CardHeader>
                        <CardTitle className="text-sm font-medium text-neutral-600 dark:text-neutral-400">
                            Free Quota
                        </CardTitle>
                    </CardHeader>
                    <CardContent>
                        <div className="flex items-center gap-2">
                            <CpuChipIcon className="w-8 h-8 text-purple-600 dark:text-purple-400" />
                            <div>
                                <div className="text-2xl font-bold text-neutral-900 dark:text-neutral-100">
                                    {config?.free_quota_hours || 0}h
                                </div>
                                <div className="text-xs text-neutral-500 dark:text-neutral-400">
                                    Per day
                                </div>
                            </div>
                        </div>
                    </CardContent>
                </Card>
            </div>

            {/* Pricing Information */}
            {config && (
                <Card>
                    <CardHeader>
                        <CardTitle>Pricing</CardTitle>
                    </CardHeader>
                    <CardContent>
                        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                            <div>
                                <div className="text-sm text-neutral-600 dark:text-neutral-400">RAM</div>
                                <div className="text-lg font-semibold text-neutral-900 dark:text-neutral-100">
                                    ${config.price_per_gb_ram.toFixed(4)}/GB/hr
                                </div>
                            </div>
                            <div>
                                <div className="text-sm text-neutral-600 dark:text-neutral-400">CPU</div>
                                <div className="text-lg font-semibold text-neutral-900 dark:text-neutral-100">
                                    ${config.price_per_cpu_core.toFixed(4)}/core/hr
                                </div>
                            </div>
                            <div>
                                <div className="text-sm text-neutral-600 dark:text-neutral-400">Disk</div>
                                <div className="text-lg font-semibold text-neutral-900 dark:text-neutral-100">
                                    ${config.price_per_gb_disk.toFixed(4)}/GB/hr
                                </div>
                            </div>
                            <div>
                                <div className="text-sm text-neutral-600 dark:text-neutral-400">Network</div>
                                <div className="text-lg font-semibold text-neutral-900 dark:text-neutral-100">
                                    ${config.price_per_gb_network.toFixed(4)}/GB
                                </div>
                            </div>
                        </div>
                    </CardContent>
                </Card>
            )}

            {/* Container Usage */}
            <Card>
                <CardHeader>
                    <CardTitle>Container Usage</CardTitle>
                </CardHeader>
                <div className="overflow-hidden">
                    <table className="min-w-full divide-y divide-neutral-200 dark:divide-neutral-700">
                        <thead className="bg-neutral-50 dark:bg-neutral-800/50">
                            <tr>
                                <th className="px-6 py-3 text-left text-xs font-medium text-neutral-500 dark:text-neutral-400 uppercase tracking-wider">
                                    Container
                                </th>
                                <th className="px-6 py-3 text-left text-xs font-medium text-neutral-500 dark:text-neutral-400 uppercase tracking-wider">
                                    Runtime
                                </th>
                                <th className="px-6 py-3 text-left text-xs font-medium text-neutral-500 dark:text-neutral-400 uppercase tracking-wider">
                                    Free Quota Used
                                </th>
                                <th className="px-6 py-3 text-left text-xs font-medium text-neutral-500 dark:text-neutral-400 uppercase tracking-wider">
                                    Billable Hours
                                </th>
                                <th className="px-6 py-3 text-left text-xs font-medium text-neutral-500 dark:text-neutral-400 uppercase tracking-wider">
                                    Cost
                                </th>
                            </tr>
                        </thead>
                        <tbody className="bg-white dark:bg-neutral-900/50 divide-y divide-neutral-200 dark:divide-neutral-700">
                            {billing?.containers?.map((container) => (
                                <tr key={container.container_id} className="hover:bg-neutral-50 dark:hover:bg-neutral-800/50">
                                    <td className="px-6 py-4 whitespace-nowrap">
                                        <div className="text-sm font-medium text-neutral-900 dark:text-neutral-100">
                                            {container.container_name}
                                        </div>
                                        <div className="text-xs text-neutral-500 dark:text-neutral-400">
                                            {container.container_id.substring(0, 8)}
                                        </div>
                                    </td>
                                    <td className="px-6 py-4 whitespace-nowrap text-sm text-neutral-900 dark:text-neutral-100">
                                        {container.total_runtime_hours.toFixed(2)}h
                                    </td>
                                    <td className="px-6 py-4 whitespace-nowrap text-sm text-neutral-900 dark:text-neutral-100">
                                        {container.free_quota_used_hours.toFixed(2)}h
                                    </td>
                                    <td className="px-6 py-4 whitespace-nowrap text-sm text-neutral-900 dark:text-neutral-100">
                                        {container.billable_hours.toFixed(2)}h
                                    </td>
                                    <td className="px-6 py-4 whitespace-nowrap text-sm font-semibold text-neutral-900 dark:text-neutral-100">
                                        ${container.total_cost.toFixed(4)}
                                    </td>
                                </tr>
                            ))}
                        </tbody>
                    </table>
                    
                    {(!billing?.containers || billing.containers.length === 0) && (
                        <div className="text-center py-12">
                            <p className="text-sm text-neutral-500 dark:text-neutral-400">No active containers</p>
                        </div>
                    )}
                </div>
            </Card>

            {/* Transaction History */}
            <Card>
                <CardHeader>
                    <CardTitle>Transaction History</CardTitle>
                </CardHeader>
                <div className="overflow-hidden">
                    <table className="min-w-full divide-y divide-neutral-200 dark:divide-neutral-700">
                        <thead className="bg-neutral-50 dark:bg-neutral-800/50">
                            <tr>
                                <th className="px-6 py-3 text-left text-xs font-medium text-neutral-500 dark:text-neutral-400 uppercase tracking-wider">
                                    Date
                                </th>
                                <th className="px-6 py-3 text-left text-xs font-medium text-neutral-500 dark:text-neutral-400 uppercase tracking-wider">
                                    Description
                                </th>
                                <th className="px-6 py-3 text-left text-xs font-medium text-neutral-500 dark:text-neutral-400 uppercase tracking-wider">
                                    Type
                                </th>
                                <th className="px-6 py-3 text-left text-xs font-medium text-neutral-500 dark:text-neutral-400 uppercase tracking-wider">
                                    Status
                                </th>
                                <th className="px-6 py-3 text-right text-xs font-medium text-neutral-500 dark:text-neutral-400 uppercase tracking-wider">
                                    Amount
                                </th>
                            </tr>
                        </thead>
                        <tbody className="bg-white dark:bg-neutral-900/50 divide-y divide-neutral-200 dark:divide-neutral-700">
                            {transactions.map((transaction) => (
                                <tr key={transaction.id} className="hover:bg-neutral-50 dark:hover:bg-neutral-800/50">
                                    <td className="px-6 py-4 whitespace-nowrap text-sm text-neutral-900 dark:text-neutral-100">
                                        {new Date(transaction.createdAt).toLocaleDateString()}
                                    </td>
                                    <td className="px-6 py-4 whitespace-nowrap text-sm text-neutral-900 dark:text-neutral-100">
                                        {transaction.description}
                                    </td>
                                    <td className="px-6 py-4 whitespace-nowrap">
                                        <span className={`inline-flex px-2 py-1 text-xs font-semibold rounded-full ${
                                            transaction.type === 'credit' 
                                                ? 'bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-400'
                                                : 'bg-red-100 text-red-800 dark:bg-red-900/30 dark:text-red-400'
                                        }`}>
                                            {transaction.type}
                                        </span>
                                    </td>
                                    <td className="px-6 py-4 whitespace-nowrap">
                                        <span className={`inline-flex px-2 py-1 text-xs font-semibold rounded-full ${
                                            transaction.status === 'completed'
                                                ? 'bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-400'
                                                : transaction.status === 'pending'
                                                ? 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900/30 dark:text-yellow-400'
                                                : 'bg-red-100 text-red-800 dark:bg-red-900/30 dark:text-red-400'
                                        }`}>
                                            {transaction.status}
                                        </span>
                                    </td>
                                    <td className="px-6 py-4 whitespace-nowrap text-right text-sm font-semibold">
                                        <span className={transaction.type === 'credit' ? 'text-green-600 dark:text-green-400' : 'text-red-600 dark:text-red-400'}>
                                            {transaction.type === 'credit' ? '+' : '-'}${transaction.amount.toFixed(2)}
                                        </span>
                                    </td>
                                </tr>
                            ))}
                        </tbody>
                    </table>
                    
                    {transactions.length === 0 && (
                        <div className="text-center py-12">
                            <p className="text-sm text-neutral-500 dark:text-neutral-400">No transactions yet</p>
                        </div>
                    )}
                </div>
            </Card>

            {/* Add Funds Modal */}
            <Modal 
                open={showAddFundsModal} 
                onClose={() => setShowAddFundsModal(false)}
                title="Add Funds"
            >
                <form onSubmit={handleAddFunds} className="space-y-4">
                    <div>
                        <Label htmlFor="amount">Amount ({currency})</Label>
                        <Input
                            id="amount"
                            type="number"
                            step="0.01"
                            min="0.01"
                            value={fundAmount}
                            onChange={(e) => setFundAmount(e.target.value)}
                            placeholder="10.00"
                            required
                        />
                        <p className="text-xs text-neutral-500 dark:text-neutral-400 mt-1">
                            Enter the amount you want to add to your balance
                        </p>
                    </div>

                    <div className="flex justify-end gap-2 pt-4">
                        <Button
                            type="button"
                            variant="ghost"
                            onClick={() => setShowAddFundsModal(false)}
                            disabled={addingFunds}
                            className="border-0"
                        >
                            Cancel
                        </Button>
                        <Button type="submit" isLoading={addingFunds}>
                            Add Funds
                        </Button>
                    </div>
                </form>
            </Modal>
        </div>
    )
}
