import React, { createContext, useContext, useEffect, useMemo, useState } from 'react'

export interface RuntimeConfig { 
  apiBaseUrl: string;
  mqtt?: {
    url: string;
    username?: string;
    password?: string;
    encryptionKey: string;
    signingKey: string;
  };
}

interface AppContextValue {
	config: RuntimeConfig
}

const AppContext = createContext<AppContextValue | undefined>(undefined)

function loadDefaultConfig(): RuntimeConfig {
	return { apiBaseUrl: '/' }
}

export const AppProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
	const [config, setConfig] = useState<RuntimeConfig>(loadDefaultConfig())

	useEffect(() => {
		fetch('/config.json')
			.then((r) => (r.ok ? r.json() : loadDefaultConfig()))
			.then((j) => setConfig({ 
				apiBaseUrl: String(j.apiBaseUrl || '/'),
				mqtt: j.mqtt 
			}))
			.catch(() => setConfig(loadDefaultConfig()))
	}, [])

	const value = useMemo<AppContextValue>(() => ({ config }), [config])
	return <AppContext.Provider value={value}>{children}</AppContext.Provider>
}

export function useApp() {
	const ctx = useContext(AppContext)
	if (!ctx) throw new Error('useApp must be used within AppProvider')
	return ctx
}

