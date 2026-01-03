import React from 'react'
import { useLocation } from 'react-router-dom'
import { gsap } from 'gsap'

interface PageTransitionProps {
    children: React.ReactNode
}

export function PageTransition({ children }: PageTransitionProps) {
    const location = useLocation()
    const [displayLocation, setDisplayLocation] = React.useState(location)
    const [displayChildren, setDisplayChildren] = React.useState(children)
    const containerRef = React.useRef<HTMLDivElement>(null)
    const isTransitioning = React.useRef(false)

    React.useEffect(() => {
        if (location.pathname !== displayLocation.pathname && !isTransitioning.current) {
            isTransitioning.current = true
            const container = containerRef.current

            if (!container) {
                setDisplayLocation(location)
                setDisplayChildren(children)
                isTransitioning.current = false
                return
            }

            // Create a timeline for smooth sequential animations
            const tl = gsap.timeline({
                onComplete: () => {
                    isTransitioning.current = false
                }
            })

            // Animate out: fade + blur + slight scale down + slide up
            tl.to(container, {
                opacity: 0,
                filter: 'blur(8px)',
                scale: 0.96,
                y: -20,
                duration: 0.25,
                ease: 'power2.in',
                onComplete: () => {
                    // Switch content while invisible
                    setDisplayLocation(location)
                    setDisplayChildren(children)
                }
            })
            // Animate in: fade + blur removal + scale back + slide to position
            .fromTo(container,
                {
                    opacity: 0,
                    filter: 'blur(8px)',
                    scale: 1.02,
                    y: 20,
                },
                {
                    opacity: 1,
                    filter: 'blur(0px)',
                    scale: 1,
                    y: 0,
                    duration: 0.3,
                    ease: 'power2.out',
                }
            )

        } else if (location.pathname === displayLocation.pathname) {
            // Same page - update children immediately without animation
            setDisplayChildren(children)
        }
    }, [location.pathname, displayLocation.pathname, children])

    return (
        <div ref={containerRef}>
            {displayChildren}
        </div>
    )
}