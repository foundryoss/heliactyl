import React from 'react'
import ReactDOM from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'
import { AppProvider } from './providers/AppProvider'
import { AuthProvider } from './providers/AuthProvider'
import { TenantProvider } from './providers/TenantProvider'
import { ThemeProvider } from './providers/ThemeProvider'
import { SidebarProvider } from './providers/SidebarProvider'
import { MQTTWrapper } from './components/MQTTWrapper'
import { AppRoutes } from './routes'
import { AlertProvider } from './components/ui/Alert'

ReactDOM.createRoot(document.getElementById('root')!).render(
	<AppProvider>
		<ThemeProvider>
			<SidebarProvider>
				<AuthProvider>
					<MQTTWrapper>
						<TenantProvider>
							<AlertProvider>
								<BrowserRouter>
									<AppRoutes />
								</BrowserRouter>
							</AlertProvider>
						</TenantProvider>
					</MQTTWrapper>
				</AuthProvider>
			</SidebarProvider>
		</ThemeProvider>
	</AppProvider>,
)

