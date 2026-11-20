import path from 'node:path'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { viteSingleFile } from 'vite-plugin-singlefile'

export default defineConfig({
  root: import.meta.dirname,
  publicDir: false,
  plugins: [react(), tailwindcss(), viteSingleFile()],
  resolve: {
    alias: {
      '@': path.resolve(import.meta.dirname, '../../../src'),
      react: path.resolve(import.meta.dirname, '../../../node_modules/react'),
      'react-dom': path.resolve(import.meta.dirname, '../../../node_modules/react-dom'),
    },
    dedupe: ['react', 'react-dom'],
  },
  build: { target: 'es2022', outDir: 'dist' },
})
