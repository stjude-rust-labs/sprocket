/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    "./src/**/*.{html,js,ts,jsx,tsx}",
    "../../../web-common/js/*.{html,js,ts,jsx,tsx}",
    "../src/**/*.rs", // for Maud templates in Rust files
  ],
  theme: {
    extend: {},
  },
  plugins: [],
}
