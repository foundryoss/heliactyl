import React, { forwardRef } from 'react'
import { cn } from '@/utils/cn'

export interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
    invalid?: boolean
}

export const Input = forwardRef<HTMLInputElement, InputProps>(function Input(
    { className, invalid, ...props },
    ref
) {
    const base =
        'w-full rounded-md border border-neutral-300 dark:border-neutral-700 bg-white/90 dark:bg-neutral-800/80 px-2 py-1.5 text-xs text-neutral-900 dark:text-neutral-100 placeholder:text-neutral-400 dark:placeholder:text-neutral-500 shadow-xs transition '
        + 'hover:border-neutral-400 dark:hover:border-neutral-500 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-neutral-900 dark:focus-visible:ring-neutral-100 focus-visible:ring-offset-2 focus-visible:ring-offset-white dark:focus-visible:ring-offset-neutral-950 '
        + 'focus-visible:border-neutral-400 dark:focus-visible:border-neutral-500 disabled:opacity-60 disabled:cursor-not-allowed'

    const error = invalid
        ? 'border-red-500 focus-visible:ring-red-600 focus-visible:border-red-600'
        : ''

    return (
        <input
            ref={ref}
            aria-invalid={invalid || undefined}
            data-invalid={invalid ? 'true' : undefined}
            className={cn(base, error, className)}
            style={{ fontSize: '13px' }}
            {...props}
        />
    )
})

export default Input


