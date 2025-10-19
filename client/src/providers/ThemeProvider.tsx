import React, { createContext, useContext, useEffect, useMemo, useState } from 'react'

type Theme = 'light' | 'dark' | 'system'

interface ThemeContextType {
  theme: Theme
  resolvedTheme: 'light' | 'dark'
  setTheme: (theme: Theme) => void
  toggleTheme: () => void
}

const LOCAL_STORAGE_KEY = 'theme'

const isBrowser = typeof window !== 'undefined'

const ThemeContext = createContext<ThemeContextType | undefined>(undefined)

function getSystemTheme(): Exclude<Theme, 'system'> {
  if (!isBrowser) {
    return 'light'
  }

  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

function applyThemeClass(theme: Theme) {
  if (!isBrowser) {
    return
  }

  const root = document.documentElement
  const resolved = theme === 'system' ? getSystemTheme() : theme

  root.classList.toggle('dark', resolved === 'dark')
  root.dataset.theme = resolved
}

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const [theme, setTheme] = useState<Theme>(() => {
    if (!isBrowser) {
      return 'system'
    }

    const saved = localStorage.getItem(LOCAL_STORAGE_KEY)
    if (saved === 'light' || saved === 'dark' || saved === 'system') {
      return saved
    }

    return 'system'
  })

  useEffect(() => {
    if (!isBrowser) {
      return
    }

    const systemTheme = getSystemTheme()
    applyThemeClass(theme)

    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)')
    const handleChange = () => {
      if (theme === 'system') {
        applyThemeClass('system')
      }
    }

    mediaQuery.addEventListener('change', handleChange)
    return () => mediaQuery.removeEventListener('change', handleChange)
  }, [theme])

  useEffect(() => {
    if (!isBrowser) {
      return
    }

    localStorage.setItem(LOCAL_STORAGE_KEY, theme)
    applyThemeClass(theme)
  }, [theme])

  const toggleTheme = () => {
    setTheme(current => {
      if (current === 'light') {
        return 'dark'
      }

      if (current === 'dark') {
        return 'system'
      }

      return 'light'
    })
  }

  const resolvedTheme = useMemo(() => (theme === 'system' ? getSystemTheme() : theme), [theme])

  const contextValue = useMemo(() => ({
    theme,
    resolvedTheme,
    setTheme,
    toggleTheme,
  }), [theme, resolvedTheme])

  return (
    <ThemeContext.Provider value={contextValue}>
      {children}
    </ThemeContext.Provider>
  )
}

export function useTheme() {
  const context = useContext(ThemeContext)
  if (context === undefined) {
    throw new Error('useTheme must be used within a ThemeProvider')
  }
  return context
}