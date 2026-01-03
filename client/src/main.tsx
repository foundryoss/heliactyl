import React from 'react'
import ReactDOM from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { AppProvider } from './providers/AppProvider'
import { AuthProvider } from './providers/AuthProvider'
import { TenantProvider } from './providers/TenantProvider'
import { ThemeProvider } from './providers/ThemeProvider'
import { SidebarProvider } from './providers/SidebarProvider'
import { MQTTWrapper } from './components/MQTTWrapper'
import { AppRoutes } from './routes'
import { AlertProvider } from './components/ui/Alert'

// Create a client
const queryClient = new QueryClient({
	defaultOptions: {
		queries: {
			refetchOnWindowFocus: false,
			retry: 1,
			staleTime: 5000,
		},
	},
})

ReactDOM.createRoot(document.getElementById('root')!).render(
	<AppProvider>
		<ThemeProvider>
			<SidebarProvider>
				<AuthProvider>
					<QueryClientProvider client={queryClient}>
						<MQTTWrapper>
							<TenantProvider>
								<AlertProvider>
									<BrowserRouter>
										<AppRoutes />
									</BrowserRouter>
								</AlertProvider>
							</TenantProvider>
						</MQTTWrapper>
					</QueryClientProvider>
				</AuthProvider>
			</SidebarProvider>
		</ThemeProvider>
	</AppProvider>,
)

