/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        background: '#090b0e',
        surface: {
          DEFAULT: '#11141a',
          card: '#151821',
          elevated: '#1a1e29',
          border: '#232836',
          'border-light': '#2e3446'
        },
        brand: {
          blue: '#4285f4',
          purple: '#9b51e0',
          green: '#34a853',
          yellow: '#fbbc04',
          red: '#ea4335'
        }
      },
      fontFamily: {
        sans: ['"Plus Jakarta Sans"', 'system-ui', 'sans-serif'],
        mono: ['"JetBrains Mono"', 'monospace']
      }
    },
  },
  plugins: [],
}
