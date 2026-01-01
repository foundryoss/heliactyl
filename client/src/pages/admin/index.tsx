import { Routes, Route, Navigate } from 'react-router-dom'
import { AdminLayout } from './AdminLayout'
import { AdminOverview } from './Overview'
import { AdminUsers } from './Users'
import { AdminNodes } from './Nodes'
import { AdminSoftware } from './Software'
import { AdminSettings } from './Settings'

export function AdminRoutes() {
	return (
		<Routes>
			<Route element={<AdminLayout />}>
				<Route index element={<AdminOverview />} />
				<Route path="users" element={<AdminUsers />} />
				<Route path="nodes" element={<AdminNodes />} />
				<Route path="software" element={<AdminSoftware />} />
				<Route path="settings" element={<AdminSettings />} />
				<Route path="*" element={<Navigate to="/admin" replace />} />
			</Route>
		</Routes>
	)
}

export { AdminLayout, AdminOverview, AdminUsers, AdminNodes, AdminSoftware, AdminSettings }
