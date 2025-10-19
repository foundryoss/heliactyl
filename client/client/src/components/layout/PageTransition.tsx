import React from 'react'
import { useLocation } from 'react-router-dom'

interface PageTransitionProps {
    children: React.ReactNode
}

export function PageTransition({ children }: PageTransitionProps) {
    const location = useLocation()
    const [isTransitioning, setIsTransitioning] = React.useState(false)
    const [displayLocation, setDisplayLocation] = React.useState(location)

    React.useEffect(() => {
        if (location.pathname !== displayLocation.pathname) {
            setIsTransitioning(true)
            
            const timer = setTimeout(() => {
                setDisplayLocation(location)
                setIsTransitioning(false)
            }, 150)

            return () => clearTimeout(timer)
        }
    }, [location, displayLocation])

    return (
        <div 
            className={`transition-all duration-300 ease-in-out ${
                isTransitioning 
                    ? 'opacity-0 blur-sm scale-[0.98]' 
                    : 'opacity-100 blur-0 scale-100'
            }`}
        >
            {children}
        </div>
    )
}