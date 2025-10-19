import React, { createContext, useContext, useEffect, useMemo, useState } from 'react'
import { cookies } from '@/utils/cookies'

interface SidebarContextType {
  isCollapsed: boolean
  setCollapsed: (collapsed: boolean) => void
  toggleCollapsed: () => void
}

const COOKIE_KEY = 'sidebar-collapsed'

const isBrowser = typeof window !== 'undefined'

const SidebarContext = createContext<SidebarContextType | undefined>(undefined)

export function SidebarProvider({ children }: { children: React.ReactNode }) {
  const [isCollapsed, setIsCollapsed] = useState<boolean>(() => {
    if (!isBrowser) {
      return false
    }

    const saved = cookies.get(COOKIE_KEY)
    return saved === 'true'
  })

  useEffect(() => {
    if (!isBrowser) {
      return
    }

    cookies.set(COOKIE_KEY, isCollapsed.toString())
  }, [isCollapsed])

  const setCollapsed = (collapsed: boolean) => {
    setIsCollapsed(collapsed)
  }

  const toggleCollapsed = () => {
    setIsCollapsed(current => !current)
  }

  const contextValue = useMemo(() => ({
    isCollapsed,
    setCollapsed,
    toggleCollapsed,
  }), [isCollapsed])

  return (
    <SidebarContext.Provider value={contextValue}>
      {children}
    </SidebarContext.Provider>
  )
}

export function useSidebar() {
  const context = useContext(SidebarContext)
  if (context === undefined) {
    throw new Error('useSidebar must be used within a SidebarProvider')
  }
  return context
}