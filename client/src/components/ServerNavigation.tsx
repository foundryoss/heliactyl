import React from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { CommandLineIcon, FolderIcon, Cog6ToothIcon } from '@heroicons/react/24/outline'

type ServerTab = 'console' | 'files' | 'settings'

interface ServerNavigationProps {
    activeTab: ServerTab
}

export function ServerNavigation({ activeTab }: ServerNavigationProps) {
    const navigate = useNavigate()
    const { serverId } = useParams<{ serverId: string }>()

    const handleNavigate = (tab: ServerTab) => {
        if (!serverId) return
        
        switch (tab) {
            case 'console':
                navigate(`/server/${serverId}`)
                break
            case 'files':
                navigate(`/server/${serverId}/files`)
                break
            case 'settings':
                navigate(`/server/${serverId}/settings`)
                break
        }
    }

    const tabs: Array<{ id: ServerTab; label: string; icon: React.ElementType }> = [
        { id: 'console', label: 'Console', icon: CommandLineIcon },
        { id: 'files', label: 'Files', icon: FolderIcon },
        { id: 'settings', label: 'Management', icon: Cog6ToothIcon },
    ]

    return (
        <div className="bg-neutral-100 dark:bg-black border border-neutral-300 dark:border-neutral-800/50 p-1">
            <nav className="flex items-stretch gap-0.5">
                {tabs.map((tab) => {
                    const Icon = tab.icon
                    const isActive = activeTab === tab.id
                    
                    return (
                        <button
                            key={tab.id}
                            onClick={() => handleNavigate(tab.id)}
                            className={`group inline-flex items-center gap-2 px-4 py-2.5 text-xs font-medium transition-all focus-visible:outline-none border-l-2 ${
                                isActive
                                    ? 'text-red-600 dark:text-red-400 bg-neutral-200/50 dark:bg-neutral-950/80 border-red-500'
                                    : 'text-neutral-600 dark:text-neutral-500 border-transparent hover:text-neutral-900 dark:hover:text-neutral-300 hover:bg-neutral-200/30 dark:hover:bg-neutral-950/50 hover:border-neutral-400 dark:hover:border-neutral-700 cursor-pointer'
                            }`}
                            style={{ fontFamily: "'Space Mono', monospace" }}
                        >
                            <Icon className="w-4 h-4" />
                            {tab.label}
                        </button>
                    )
                })}
            </nav>
        </div>
    )
}
