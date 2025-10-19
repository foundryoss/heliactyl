import React, { forwardRef } from 'react'
import { cn } from '@/utils/cn'

export interface TextareaProps extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {
    invalid?: boolean
}

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaProps>(function Textarea(
    { className, invalid, ...props },
    ref
) {
    const base = 'w-full rounded-md border border-gray-300 bg-white px-3 py-2 text-sm text-gray-900 placeholder:text-gray-400 outline-none transition-shadow focus:ring-2 focus:ring-blue-500 focus:border-blue-500 disabled:opacity-60 disabled:cursor-not-allowed'
    const error = invalid ? 'border-red-400 focus:ring-red-500 focus:border-red-500' : ''
    return <textarea ref={ref} className={cn(base, error, className)} {...props} />
})

export default Textarea


