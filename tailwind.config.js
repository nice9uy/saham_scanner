/** @type {import('tailwindcss').Config} */
module.exports = {
    content: [
      "./templates/**/*.html",
      "./node_modules/flowbite/**/*.js"
    ],
    theme: {
      extend: {
        zIndex: {
          '100': '100',
        },
      },
    },
    plugins: [],
    darkMode: 'class',
    safelist: ['bg-blue-700'],
  }