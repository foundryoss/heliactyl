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
					'absolute inset-0 bg-black/30 dark:bg-black/60 backdrop-blur-sm cursor-pointer',
					!open && isAnimating ? 'animate-backdrop-out' : 'animate-backdrop-in'
				)}
				onClick={onClose}
			/>

			{/* Panel */}
			<div className="absolute inset-0 grid place-items-center p-8 pointer-events-none">
				<div
					className={cn(
						'relative w-[480px] max-w-[96vw] pointer-events-auto origin-center bg-neutral-100 dark:bg-black border border-neutral-300 dark:border-neutral-800/50 shadow-2xl cursor-auto',
						!open && isAnimating ? 'animate-modal-out' : 'animate-modal-in',
						className
					)}
					onClick={(e) => e.stopPropagation()}
				>
				{title && (
					<div className="px-5 py-4 border-b border-neutral-300 dark:border-neutral-800/50">
						<div className="flex items-center gap-3">
							<h3 
								className="text-lg text-neutral-900 dark:text-neutral-100 tracking-wider"
								style={{ fontFamily: "'Seven Segment', sans-serif" }}
							>
								{title}
							</h3>
							<div className="flex-1 h-px bg-neutral-300 dark:bg-neutral-800"></div>
						</div>
						{subtitle && (
							<div 
								className="text-[10px] text-neutral-500 dark:text-neutral-600 uppercase tracking-[0.2em] mt-1"
								style={{ fontFamily: "'Space Mono', monospace" }}
							>
								{subtitle}
							</div>
						)}
					</div>
				)}
					<div className={cn("p-5", !title && "pt-5")}>
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


