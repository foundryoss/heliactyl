import React from 'react'
import { cn } from '@/utils/cn'

export interface CardProps extends React.HTMLAttributes<HTMLDivElement> {}
export interface CardHeaderProps extends React.HTMLAttributes<HTMLDivElement> {}
export interface CardTitleProps extends React.HTMLAttributes<HTMLHeadingElement> {}
export interface CardDescriptionProps extends React.HTMLAttributes<HTMLParagraphElement> {}
export interface CardContentProps extends React.HTMLAttributes<HTMLDivElement> {}
export interface CardFooterProps extends React.HTMLAttributes<HTMLDivElement> {}

export function Card({ className, ...props }: CardProps) {
    return <div className={cn('rounded-lg border border-neutral-200 dark:border-transparent bg-white dark:bg-neutral-800', className)} {...props} />
}

export function CardHeader({ className, ...props }: CardHeaderProps) {
    return <div className={cn('px-8 pt-8 pb-4', className)} {...props} />
}

export function CardTitle({ className, ...props }: CardTitleProps) {
    return <h3 className={cn('text-lg font-semibold text-neutral-900 dark:text-neutral-100 tracking-tight', className)} {...props} />
}

export function CardDescription({ className, ...props }: CardDescriptionProps) {
    return <p className={cn('mt-1 text-sm text-neutral-700 dark:text-neutral-300 tracking-medium', className)} {...props} />
}

export function CardContent({ className, ...props }: CardContentProps) {
    return <div className={cn('px-8 pb-8 pt-2', className)} {...props} />
}

export function CardFooter({ className, ...props }: CardFooterProps) {
    return <div className={cn('px-8 pb-8 pt-4', className)} {...props} />
}

export default Card


