import React from 'react'
import { useSidebar } from '@/providers/SidebarProvider'
import { cn } from '@/utils/cn'
import Tooltip from '@/components/ui/Tooltip'

interface FloatingSidebarToggleProps {
  className?: string
}

export function SidebarGripHandle({ className }: FloatingSidebarToggleProps) {
  const { isCollapsed, toggleCollapsed } = useSidebar()
  const [isMobile, setIsMobile] = React.useState(false)

  React.useEffect(() => {
    const checkMobile = () => setIsMobile(window.innerWidth <= 768)
    checkMobile()
    const handleResize = () => checkMobile()
    window.addEventListener('resize', handleResize)
    return () => window.removeEventListener('resize', handleResize)
  }, [])

  // Only show the handle when the sidebar is collapsed
  if (!isCollapsed) return null

  const handle = (
    <button
      onClick={toggleCollapsed}
      className={cn(
        // Position: center-left, slight inset so it "pushes" content visually
        'fixed left-0 top-1/2 -translate-y-1/2 z-50',
        // Shape and size: slightly smaller grip-like handle
        'h-24 w-[14px] md:w-[12px]',
        // Beveled right edge for a diagonal cut
        '[clip-path:polygon(0_0,calc(100%_-_8px)_0,100%_8px,100%_calc(100%_-_8px),calc(100%_-_8px)_100%,0_100%)]',
        // Borders and surface
        'border border-neutral-200/60 dark:border-neutral-700/60 border-l-0',
        'backdrop-blur-md bg-white/90 dark:bg-neutral-900/85',
        // Textures: thinner, denser diagonal stripes + subtle sheen (less bright in dark)
        "[background-image:repeating-linear-gradient(135deg,rgba(0,0,0,0.06)_0px,rgba(0,0,0,0.06)_2px,transparent_2px,transparent_5px),linear-gradient(180deg,rgba(255,255,255,0.35),rgba(255,255,255,0.0))]",
        "dark:[background-image:repeating-linear-gradient(135deg,rgba(255,255,255,0.10)_0px,rgba(255,255,255,0.10)_2px,transparent_2px,transparent_5px),linear-gradient(180deg,rgba(255,255,255,0.06),rgba(255,255,255,0.0))]",
        // Interaction
        'transition-[width,box-shadow,background-color] duration-200 ease-out hover:w-[18px] md:hover:w-[14px] shadow-lg hover:shadow-xl focus:outline-none',
        'focus:ring-2 focus:ring-neutral-900 dark:focus:ring-neutral-100 focus:ring-offset-2 focus:ring-offset-white dark:focus:ring-offset-neutral-950',
        'touch-pan-y',
        className
      )}
      aria-label="Open sidebar"
    >
      <span className="sr-only">Open sidebar</span>
    </button>
  )

  return isMobile ? (
    handle
  ) : (
    <Tooltip content="Open sidebar" placement="right" offset={10}>
      {handle}
    </Tooltip>
  )
}