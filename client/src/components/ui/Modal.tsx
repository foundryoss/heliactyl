import React, { useEffect, useState } from 'react'
import { createPortal } from 'react-dom'
import { cn } from '@/utils/cn'

export interface ModalProps {
	open: boolean
	onClose: () => void
	title?: string
	subtitle?: string
	children: React.ReactNode
	className?: string
    zIndex?: number
}

export function Modal({ open, onClose, title, subtitle, children, className, zIndex }: ModalProps) {
	const [isVisible, setIsVisible] = useState(false)
	const [isAnimating, setIsAnimating] = useState(false)

	useEffect(() => {
		function onKey(e: KeyboardEvent) { 
			if (e.key === 'Escape' && open) onClose() 
		}
		if (open) document.addEventListener('keydown', onKey)
		return () => document.removeEventListener('keydown', onKey)
	}, [open, onClose])

	useEffect(() => {
		if (open) {
			setIsVisible(true)
			setIsAnimating(true)
			// Small delay to trigger animation
			requestAnimationFrame(() => {
				requestAnimationFrame(() => {
					setIsAnimating(false)
				})
			})
		} else if (isVisible) {
			setIsAnimating(true)
			// Wait for animation to complete before hiding
			setTimeout(() => {
				setIsVisible(false)
				setIsAnimating(false)
			}, 150)
		}
	}, [open, isVisible])

	if (!isVisible) return null

	const content = (
		<div
			className="fixed inset-0 pointer-events-auto"
			style={{ zIndex: zIndex ?? 90 }}
			aria-hidden={!open}
		>
			{/* Backdrop */}
			<div
				className={cn(
					'absolute inset-0 bg-black/20 dark:bg-black/40 backdrop-blur-md cursor-pointer',
					!open && isAnimating ? 'animate-backdrop-out' : 'animate-backdrop-in'
				)}
				onClick={onClose}
			/>

			{/* Panel */}
			<div className="absolute inset-0 grid place-items-center p-8 pointer-events-none">
				<div
					className={cn(
						'relative w-[480px] max-w-[96vw] pointer-events-auto origin-center rounded-lg bg-white dark:bg-neutral-900 shadow-2xl cursor-auto',
						!open && isAnimating ? 'animate-modal-out' : 'animate-modal-in',
						className
					)}
					onClick={(e) => e.stopPropagation()}
				>
				{title && (
					<div className="px-4 py-3 border-b border-neutral-200/70 dark:border-transparent">
							<div className="text-sm font-semibold text-neutral-900 dark:text-neutral-100">{title}</div>
							{subtitle && <div className="hidden text-[11px] text-neutral-500 dark:text-neutral-400 tracking-widest uppercase">{subtitle}</div>}
						</div>
					)}
					<div className={cn("p-4", !title && "pt-4")}>
						{children}
					</div>
				</div>
			</div>
		</div>
	)

	if (typeof document === 'undefined') return content
	return createPortal(content, document.body)
}

export default Modal


