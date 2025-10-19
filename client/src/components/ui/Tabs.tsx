import React, { createContext, useContext, useState } from 'react'
import { cn } from '@/utils/cn'

interface TabsContextType {
    activeTab: string
    setActiveTab: (tab: string) => void
}

const TabsContext = createContext<TabsContextType | null>(null)

function useTabsContext() {
    const context = useContext(TabsContext)
    if (!context) {
        throw new Error('Tabs components must be used within a Tabs provider')
    }
    return context
}

interface TabsProps {
    defaultValue: string
    value?: string
    onValueChange?: (value: string) => void
    children: React.ReactNode
    className?: string
}

export function Tabs({ defaultValue, value, onValueChange, children, className }: TabsProps) {
    const [internalValue, setInternalValue] = useState(defaultValue)
    
    const activeTab = value ?? internalValue
    const setActiveTab = (tab: string) => {
        if (value === undefined) {
            setInternalValue(tab)
        }
        onValueChange?.(tab)
    }

    return (
        <TabsContext.Provider value={{ activeTab, setActiveTab }}>
            <div className={cn('w-full', className)}>
                {children}
            </div>
        </TabsContext.Provider>
    )
}

interface TabsListProps {
    children: React.ReactNode
    className?: string
}

export function TabsList({ children, className }: TabsListProps) {
    return (
        <div className={cn('inline-flex h-4 items-center justify-center rounded-full bg-neutral-100 dark:bg-neutral-800 p-1 text-neutral-500 dark:text-neutral-400', className)}>
            {children}
        </div>
    )
}

interface TabsTriggerProps {
    value: string
    children: React.ReactNode
    className?: string
}

export function TabsTrigger({ value, children, className }: TabsTriggerProps) {
    const { activeTab, setActiveTab } = useTabsContext()
    const isActive = activeTab === value

    return (
        <button
            type="button"
            onClick={() => setActiveTab(value)}
            data-state={isActive ? 'active' : 'inactive'}
            style={{ fontSize: '13px' }}
            className={cn(
                'inline-flex items-center cursor-pointer justify-center whitespace-nowrap rounded-full px-2 py-0.5 text-xs font-semibold ring-offset-white dark:ring-offset-neutral-950 transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-neutral-400 dark:focus-visible:ring-neutral-500 focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50',
                isActive 
                    ? 'bg-white dark:bg-neutral-700 text-neutral-900 dark:text-neutral-100' 
                    : 'text-neutral-600 dark:text-neutral-400 hover:text-neutral-900 dark:hover:text-neutral-100',
                className
            )}
        >
            {children}
        </button>
    )
}

interface TabsContentProps {
    value: string
    children: React.ReactNode
    className?: string
}

export function TabsContent({ value, children, className }: TabsContentProps) {
    const { activeTab } = useTabsContext()
    
    if (activeTab !== value) {
        return null
    }

    return (
        <div className={cn('mt-2 ring-offset-white dark:ring-offset-neutral-950 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-neutral-400 dark:focus-visible:ring-neutral-500 focus-visible:ring-offset-2', className)}>
            {children}
        </div>
    )
}