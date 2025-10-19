import React from 'react'
import { cn } from '@/utils/cn'

export function H1({ className, ...props }: React.HTMLAttributes<HTMLHeadingElement>) {
    return <h1 className={cn('text-lg font-semibold text-gray-900', className)} {...props} />
}

export function H2({ className, ...props }: React.HTMLAttributes<HTMLHeadingElement>) {
    return <h2 className={cn('text-md font-semibold text-gray-900', className)} {...props} />
}

export function Muted({ className, ...props }: React.HTMLAttributes<HTMLParagraphElement>) {
    return <p className={cn('text-xs text-gray-600', className)} {...props} />
}

export function Small({ className, ...props }: React.HTMLAttributes<HTMLSpanElement>) {
    return <span className={cn('text-[11px] text-gray-500', className)} {...props} />
}


