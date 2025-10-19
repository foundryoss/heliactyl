import React from 'react'
import { cn } from '@/utils/cn'

export interface SpinnerProps {
    size?: 'sm' | 'md' | 'lg' | 'xl'
    className?: string
    children?: React.ReactNode
}

const sizeClasses = {
    sm: 'h-4 w-4',
    md: 'h-6 w-6', 
    lg: 'h-8 w-8',
    xl: 'h-12 w-12'
} as const

export function Spinner({ size = 'md', className, children }: SpinnerProps) {
    return (
        <div className="flex flex-col items-center justify-center gap-3">
            <span
                className={cn(
                    'inline-block rounded-full border-2 border-current border-t-transparent animate-spin',
                    sizeClasses[size],
                    className
                )}
                role="status"
                aria-live="polite"
            />
            {children && (
                <div className="text-sm text-gray-600">
                    {children}
                </div>
            )}
            <span className="sr-only">Loading</span>
        </div>
    )
}

export default Spinner