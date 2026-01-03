import { Routes, Route, Navigate } from 'react-router-dom'
import { ConsolePage } from './Console'
import { FilesPage } from './Files'
import { ServerCreatePreflightPage } from './CreatePreflight'

export { ServerCreatePreflightPage }

export function ServerRoutes() {
    return (
        <Routes>
            <Route index element={<ConsolePage />} />
            <Route path="console" element={<ConsolePage />} />
            <Route path="files" element={<FilesPage />} />
            <Route path="*" element={<Navigate to="" replace />} />
        </Routes>
    )
}