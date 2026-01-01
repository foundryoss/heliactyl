import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'node:path'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
	plugins: [react(), tailwindcss()],
	server: {
		port: 5173,
		strictPort: false,
		proxy: {
			'/api/auth': { target: 'http://localhost:8787', changeOrigin: true },
			'/api/user': { target: 'http://localhost:8787', changeOrigin: true },
			'/api/tenants': { target: 'http://localhost:8787', changeOrigin: true },
			'/api/admin': { target: 'http://localhost:8787', changeOrigin: true },
			'/api/core': { target: 'http://localhost:8787', changeOrigin: true },
			'/api/wallet': { target: 'http://localhost:8787', changeOrigin: true },
			'/api/billing': { target: 'http://localhost:8787', changeOrigin: true },
		},
	},
	resolve: {
		alias: {
			'@': path.resolve(__dirname, 'src'),
		},
	},
})
