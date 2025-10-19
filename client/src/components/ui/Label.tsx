import React, { forwardRef } from 'react'
import { cn } from '@/utils/cn'

export interface LabelProps extends React.LabelHTMLAttributes<HTMLLabelElement> {
    hint?: string
    requiredMark?: boolean
}

export const Label = forwardRef<HTMLLabelElement, LabelProps>(function Label(
    { className, children, hint, requiredMark, ...props },
    ref
) {
    return (
        <label ref={ref} className={cn('mb-1.5 block text-sm font-semibold text-neutral-900 dark:text-neutral-100', className)} {...props}>
            <span className="inline-flex items-center gap-1.5">
                {children}
                {requiredMark && <span className="text-red-600" aria-hidden>*</span>}
            </span>
            {hint && <span className="mt-0.5 block text-xs text-neutral-500 dark:text-neutral-400">{hint}</span>}
        </label>
    )
})

export default Label


