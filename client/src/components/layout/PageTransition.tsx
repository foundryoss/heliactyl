import React from 'react'
import { useLocation } from 'react-router-dom'

interface PageTransitionProps {
    children: React.ReactNode
}

export function PageTransition({ children }: PageTransitionProps) {
    const location = useLocation()
    const [phase, setPhase] = React.useState<'idle' | 'blurring-out' | 'blurring-in'>('idle')
    const [displayLocation, setDisplayLocation] = React.useState(location)
    const [displayChildren, setDisplayChildren] = React.useState(children)
    const timersRef = React.useRef<NodeJS.Timeout[]>([])

    React.useEffect(() => {
        // Clear any existing timers
        timersRef.current.forEach(timer => clearTimeout(timer))
        timersRef.current = []

        if (location.pathname !== displayLocation.pathname) {
            // Start blur out phase
            setPhase('blurring-out')
            
            const timer1 = setTimeout(() => {
                // Switch content and start blur in
                setDisplayLocation(location)
                setDisplayChildren(children)
                setPhase('blurring-in')
                
                const timer2 = setTimeout(() => {
                    setPhase('idle')
                }, 150)
                
                timersRef.current.push(timer2)
            }, 150)

            timersRef.current.push(timer1)
        } else {
            // Same page - update children immediately
            setDisplayChildren(children)
        }

        return () => {
            timersRef.current.forEach(timer => clearTimeout(timer))
            timersRef.current = []
        }
    }, [location.pathname, displayLocation.pathname, children])

    const isBlurred = phase === 'blurring-out'

    return (
        <div 
            className={`transition-all duration-150 ease-in-out ${
                isBlurred
                    ? 'opacity-0 blur-sm scale-[0.98]' 
                    : 'opacity-100 blur-0 scale-100'
            }`}
        >
            {displayChildren}
        </div>
    )
}