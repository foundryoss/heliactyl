import React, { useState, useRef, useEffect } from 'react'
import { cn } from '@/utils/cn'
import { ChevronDownIcon, CheckIcon } from '@heroicons/react/24/outline'

export interface SelectOption {
    value: string
    label: string
    disabled?: boolean
}

export interface SelectProps {
    value: string
    onChange: (value: string) => void
    options: SelectOption[]
    placeholder?: string
    disabled?: boolean
    invalid?: boolean
    className?: string
    id?: string
    required?: boolean
}

export const Select = React.forwardRef<HTMLButtonElement, SelectProps>(function Select(
    { value, onChange, options, placeholder = 'Select...', disabled, invalid, className, id, required, ...props },
    ref
) {
    const [isOpen, setIsOpen] = useState(false)
    const containerRef = useRef<HTMLDivElement>(null)

    const selectedOption = options.find(opt => opt.value === value)

    useEffect(() => {
        const handleClickOutside = (event: MouseEvent) => {
            if (containerRef.current && !containerRef.current.contains(event.target as Node)) {
                setIsOpen(false)
            }
        }

        if (isOpen) {
            document.addEventListener('mousedown', handleClickOutside)
        }

        return () => {
            document.removeEventListener('mousedown', handleClickOutside)
        }
    }, [isOpen])

    const handleSelect = (optionValue: string) => {
        onChange(optionValue)
        setIsOpen(false)
    }

    const base =
        'w-full rounded-md border border-neutral-300 dark:border-neutral-700 bg-white/90 dark:bg-neutral-800/80 px-2 py-1.5 text-xs text-neutral-900 dark:text-neutral-100 shadow-xs transition '
        + 'hover:border-neutral-400 dark:hover:border-neutral-500 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-neutral-900 dark:focus-visible:ring-neutral-100 focus-visible:ring-offset-2 focus-visible:ring-offset-white dark:focus-visible:ring-offset-neutral-950 '
        + 'focus-visible:border-neutral-400 dark:focus-visible:border-neutral-500 disabled:opacity-60 disabled:cursor-not-allowed'

    const error = invalid
        ? 'border-red-500 focus-visible:ring-red-600 focus-visible:border-red-600'
        : ''

    return (
        <div ref={containerRef} className="relative">
            <button
                ref={ref}
                type="button"
                id={id}
                disabled={disabled}
                aria-expanded={isOpen}
                aria-haspopup="listbox"
                aria-required={required}
                onClick={() => !disabled && setIsOpen(!isOpen)}
                className={cn(
                    base,
                    error,
                    'flex items-center justify-between text-left',
                    className
                )}
                style={{ fontSize: '13px' }}
                {...props}
            >
                <span className={selectedOption ? 'text-neutral-900 dark:text-neutral-100' : 'text-neutral-400 dark:text-neutral-500'}>
                    {selectedOption ? selectedOption.label : placeholder}
                </span>
                <ChevronDownIcon 
                    className={cn(
                        'w-4 h-4 text-neutral-400 dark:text-neutral-500 transition-transform',
                        isOpen && 'rotate-180'
                    )} 
                />
            </button>

            {isOpen && (
                <div className="absolute z-50 w-full mt-1 bg-white dark:bg-neutral-800 border border-neutral-300 dark:border-neutral-700 rounded-md shadow-lg max-h-60 overflow-auto">
                    {options.map((option) => (
                        <button
                            key={option.value}
                            type="button"
                            disabled={option.disabled}
                            onClick={() => !option.disabled && handleSelect(option.value)}
                            className={cn(
                                'w-full px-2 py-1.5 text-left text-xs transition-colors flex items-center justify-between',
                                'hover:bg-neutral-100 dark:hover:bg-neutral-700',
                                'focus:bg-neutral-100 dark:focus:bg-neutral-700 focus:outline-none',
                                'disabled:opacity-50 disabled:cursor-not-allowed',
                                value === option.value && 'bg-neutral-50 dark:bg-neutral-700/50'
                            )}
                            style={{ fontSize: '13px' }}
                        >
                            <span className="text-neutral-900 dark:text-neutral-100">
                                {option.label}
                            </span>
                            {value === option.value && (
                                <CheckIcon className="w-4 h-4 text-neutral-600 dark:text-neutral-400" />
                            )}
                        </button>
                    ))}
                </div>
            )}
        </div>
    )
})

export default Select


