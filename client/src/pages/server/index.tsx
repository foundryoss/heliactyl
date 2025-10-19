import { Routes, Route, Navigate } from 'react-router-dom'
import { ConsolePage } from './Console'

export function ServerRoutes() {
    return (
        <Routes>
            <Route index element={<ConsolePage />} />
            <Route path="console" element={<ConsolePage />} />
            <Route path="*" element={<Navigate to="" replace />} />
        </Routes>
    )
}