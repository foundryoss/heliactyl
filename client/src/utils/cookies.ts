const isBrowser = typeof window !== 'undefined'

export const cookies = {
  get: (name: string): string | null => {
    if (!isBrowser) return null
    
    const value = `; ${document.cookie}`
    const parts = value.split(`; ${name}=`)
    if (parts.length === 2) {
      const cookieValue = parts.pop()?.split(';').shift()
      return cookieValue ? decodeURIComponent(cookieValue) : null
    }
    return null
  },

  set: (name: string, value: string, days: number = 365): void => {
    if (!isBrowser) return
    
    const date = new Date()
    date.setTime(date.getTime() + (days * 24 * 60 * 60 * 1000))
    const expires = `expires=${date.toUTCString()}`
    document.cookie = `${name}=${encodeURIComponent(value)};${expires};path=/;SameSite=Lax`
  },

  remove: (name: string): void => {
    if (!isBrowser) return
    
    document.cookie = `${name}=;expires=Thu, 01 Jan 1970 00:00:00 GMT;path=/;SameSite=Lax`
  }
}