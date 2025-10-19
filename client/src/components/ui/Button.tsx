import React, { forwardRef, useEffect, useMemo, useRef, useState } from 'react'
import { cn } from '@/utils/cn'

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
	isLoading?: boolean
	busyText?: string
	variant?: 'primary' | 'secondary' | 'destructive' | 'ghost' | 'link'
	size?: 'sm' | 'md' | 'lg'
    leftIcon?: React.ReactNode
    rightIcon?: React.ReactNode
}

const variantClasses: Record<NonNullable<ButtonProps['variant']>, string> = {
	primary: 'bg-neutral-900 dark:bg-neutral-100 text-white dark:text-neutral-900 hover:bg-neutral-800 dark:hover:bg-neutral-200 disabled:opacity-60',
	secondary: 'bg-neutral-100 dark:bg-neutral-800 outline-transparent text-neutral-900 dark:text-neutral-100 hover:bg-neutral-200 dark:hover:bg-neutral-700 disabled:opacity-60',
	destructive: 'bg-red-500 dark:bg-red-600 text-white hover:bg-red-600 dark:hover:bg-red-700 disabled:opacity-60',
	ghost: 'bg-transparent outline-none text-neutral-900 dark:text-neutral-100 hover:bg-neutral-100/80 dark:hover:bg-neutral-800/80 disabled:opacity-60',
	link: 'bg-transparent text-blue-600 dark:text-blue-400 underline underline-offset-2 disabled:opacity-60',
}

const sizeClasses: Record<NonNullable<ButtonProps['size']>, string> = {
	sm: 'text-xs px-3 py-1.5',
	md: 'text-xs px-4 py-2',
	lg: 'text-xs px-5 py-2.5',
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
    { isLoading = false, busyText = 'Loading...', disabled, children, variant = 'primary', size = 'md', leftIcon, rightIcon, className, ...rest },
	ref
) {
    // Enforce a minimum spinner visibility of 500ms to avoid flicker
    const [isSpinnerVisible, setIsSpinnerVisible] = useState<boolean>(isLoading)
    const loadingStartAtRef = useRef<number | null>(isLoading ? Date.now() : null)

    useEffect(() => {
        if (isLoading) {
            setIsSpinnerVisible(true)
            loadingStartAtRef.current = Date.now()
            return
        }

        const startedAt = loadingStartAtRef.current
        const elapsedMs = startedAt ? Date.now() - startedAt : Infinity
        const remainingMs = Math.max(0, 500 - elapsedMs)

        const timeoutId = window.setTimeout(() => {
            setIsSpinnerVisible(false)
            loadingStartAtRef.current = null
        }, remainingMs)

        return () => window.clearTimeout(timeoutId)
    }, [isLoading])

	const classes = cn(
        'transition-all focus:outline-2 focus:outline-offset-2 outline outline-neutral-900 dark:outline-neutral-100 focus:outline-neutral-800 dark:focus:outline-neutral-200 rounded-md cursor-pointer font-semibold tracking-tight inline-flex items-center justify-center whitespace-nowrap gap-1.5',
		variantClasses[variant],
		sizeClasses[size],
		className
	)

    const spinnerSizeClasses = useMemo(() => {
        switch (size) {
            case 'sm':
                return 'h-4 w-4'
            case 'lg':
                return 'h-5 w-5'
            default:
                return 'h-4 w-4'
        }
    }, [size])

	return (
        <button
            className={classes}
            ref={ref}
            disabled={!!disabled || isLoading}
            aria-busy={isSpinnerVisible}
            {...rest}
        >
            <span className="relative inline-flex items-center justify-center">
                {/* Content */}
                <span
                    className={cn(
                        'flex items-center gap-1.5 transition duration-200 ease-out will-change-transform',
                        isSpinnerVisible ? 'opacity-0 blur-[2px] scale-95' : 'opacity-100 blur-0 scale-100'
                    )}
                    aria-hidden={isSpinnerVisible}
                >
                    {leftIcon}
                    <span className="font-semibold text-sm">{children}</span>
                    {rightIcon}
                </span>

                {/* Spinner overlay */}
                <span
                    className={cn(
                        'absolute inset-0 flex items-center justify-center transition duration-200 ease-out',
                        isSpinnerVisible ? 'opacity-100 blur-0 scale-100' : 'opacity-0 blur-[2px] scale-95 pointer-events-none'
                    )}
                    role="status"
                    aria-live="polite"
                >
                    <span
                        className={cn(
                            'inline-block rounded-full border-2 border-current border-t-transparent animate-spin',
                            spinnerSizeClasses
                        )}
                    />
                    <span className="sr-only">{busyText || 'Loading'}</span>
                </span>
            </span>
        </button>
	)
})

export default Button


