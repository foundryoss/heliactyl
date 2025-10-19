import React from 'react'
import { createPortal } from 'react-dom'
import { cn } from '@/utils/cn'

export interface TooltipProps {
  children: React.ReactElement
  content: React.ReactNode
  placement?: 'top' | 'bottom' | 'left' | 'right'
  delay?: number
  offset?: number
  disabled?: boolean
  className?: string
}

export function Tooltip({
  children,
  content,
  placement = 'top',
  delay = 500,
  offset = 8,
  disabled = false,
  className
}: TooltipProps) {
  const [isVisible, setIsVisible] = React.useState(false)
  const [phase, setPhase] = React.useState<'idle' | 'in' | 'out'>('idle')
  const [position, setPosition] = React.useState({ x: 0, y: 0 })
  const timeoutRef = React.useRef<NodeJS.Timeout | null>(null)
  const childRef = React.useRef<HTMLElement>(null)

  const showTooltip = React.useCallback(() => {
    if (disabled || !content) return
    
    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current)
    }

    timeoutRef.current = setTimeout(() => {
      if (childRef.current) {
        const rect = childRef.current.getBoundingClientRect()
        const scrollX = window.scrollX
        const scrollY = window.scrollY
        
        let x = 0
        let y = 0
        
        switch (placement) {
          case 'top':
            x = rect.left + scrollX + rect.width / 2
            y = rect.top + scrollY - offset
            break
          case 'bottom':
            x = rect.left + scrollX + rect.width / 2
            y = rect.bottom + scrollY + offset
            break
          case 'left':
            x = rect.left + scrollX - offset
            y = rect.top + scrollY + rect.height / 2
            break
          case 'right':
            x = rect.right + scrollX + offset
            y = rect.top + scrollY + rect.height / 2
            break
        }
        
        setPosition({ x, y })
        setIsVisible(true)
        setPhase('in')
        // Clear phase after animation duration
        setTimeout(() => setPhase('idle'), 200)
      }
    }, delay)
  }, [disabled, content, placement, offset, delay])

  const hideTooltip = React.useCallback(() => {
    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current)
      timeoutRef.current = null
    }
    
    if (isVisible) {
      setPhase('out')
      setTimeout(() => {
        setIsVisible(false)
        setPhase('idle')
      }, 150)
    }
  }, [isVisible])

  React.useEffect(() => {
    return () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current)
      }
    }
  }, [])

  const clonedChild = React.cloneElement(children, {
    ref: childRef,
    onMouseEnter: (e: React.MouseEvent) => {
      children.props.onMouseEnter?.(e)
      showTooltip()
    },
    onMouseLeave: (e: React.MouseEvent) => {
      children.props.onMouseLeave?.(e)
      hideTooltip()
    },
    onFocus: (e: React.FocusEvent) => {
      children.props.onFocus?.(e)
      showTooltip()
    },
    onBlur: (e: React.FocusEvent) => {
      children.props.onBlur?.(e)
      hideTooltip()
    }
  })

  const getTransformOrigin = () => {
    switch (placement) {
      case 'top': return 'bottom center'
      case 'bottom': return 'top center'
      case 'left': return 'right center'
      case 'right': return 'left center'
      default: return 'bottom center'
    }
  }

  const getTransform = () => {
    switch (placement) {
      case 'top': return 'translate(-50%, -100%)'
      case 'bottom': return 'translate(-50%, 0%)'
      case 'left': return 'translate(-100%, -50%)'
      case 'right': return 'translate(0%, -50%)'
      default: return 'translate(-50%, -100%)'
    }
  }

  const tooltipElement = isVisible && content && typeof document !== 'undefined' ? (
    createPortal(
      <div
        className={cn(
          'fixed z-[9999] pointer-events-none select-none',
          'px-2 py-1.5 text-xs font-medium',
          'bg-neutral-900 dark:bg-neutral-100',
          'text-white dark:text-neutral-900',
          'rounded-md shadow-lg border border-neutral-800 dark:border-neutral-200',
          'max-w-xs whitespace-nowrap',
          // Animations
          phase === 'out' && 'animate-tooltip-out',
          phase === 'in' && 'animate-tooltip-in',
          className
        )}
        style={{
          left: position.x,
          top: position.y,
          // Provide a stable transform via CSS var used by keyframes to avoid a first-frame shift
          ['--tooltip-transform' as any]: getTransform(),
          transform: 'var(--tooltip-transform)' as any,
          transformOrigin: getTransformOrigin()
        }}
        role="tooltip"
      >
        {content}
        {/* Arrow */}
        <div 
          className={cn(
            'absolute w-2 h-2 rotate-45',
            'bg-neutral-900 dark:bg-neutral-100',
            'border-neutral-800 dark:border-neutral-200',
            placement === 'top' && 'top-full left-1/2 -translate-x-1/2 -translate-y-1/2 border-r border-b',
            placement === 'bottom' && 'bottom-full left-1/2 -translate-x-1/2 translate-y-1/2 border-l border-t',
            placement === 'left' && 'left-full top-1/2 -translate-x-1/2 -translate-y-1/2 border-t border-r',
            placement === 'right' && 'right-full top-1/2 translate-x-1/2 -translate-y-1/2 border-b border-l'
          )}
        />
      </div>,
      document.body
    )
  ) : null

  return (
    <>
      {clonedChild}
      {tooltipElement}
    </>
  )
}

export default Tooltip