/** @type {import('tailwindcss').Config} */
export default {
  darkMode: 'class',
  content: ['./src/**/*.{html,js,svelte,ts}'],
  theme: {
    extend: {
      colors: {
        // Paleta "DevFlow Dark Blue" (DESIGN.md). Remapeamos la escala `zinc`
        // a navy para que todas las clases `dark:bg-zinc-*` existentes adopten
        // el tema sin tocar componentes. Tonos claros = texto, oscuros = fondos.
        zinc: {
          50: '#eef1fc',
          100: '#dae2fd', // on-surface (texto principal)
          200: '#c2c6d6', // on-surface-variant
          300: '#abb0c2',
          400: '#9398a9', // texto atenuado
          500: '#8c909f', // outline / texto secundario
          600: '#5f6577',
          700: '#2d3449', // surface-variant (bordes)
          800: '#222a3d', // surface-container-high (hover, botón secundario)
          900: '#0b1326', // surface (sidebar, inputs)
          950: '#060e20' // surface-container-lowest (fondo app)
        },
        // Azul eléctrico para acciones primarias y estados de foco.
        primary: {
          DEFAULT: '#4d8eff',
          fg: '#ffffff',
          400: '#adc6ff',
          500: '#4d8eff',
          600: '#3b82f6'
        }
      }
    }
  },
  plugins: []
};
