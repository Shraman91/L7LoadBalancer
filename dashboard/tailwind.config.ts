import type { Config } from "tailwindcss";

const config: Config = {
  content: [
    "./pages/**/*.{js,ts,jsx,tsx,mdx}",
    "./components/**/*.{js,ts,jsx,tsx,mdx}",
    "./app/**/*.{js,ts,jsx,tsx,mdx}",
  ],
  theme: {
    extend: {
      colors: {
        background: "#FAFAFA",
        surface: "#FFFFFF",
        "surface-subtle": "#F4F4F5",
        border: {
          DEFAULT: "#E4E4E7",
          strong: "#D4D4D8",
        },
        foreground: {
          DEFAULT: "#0A0A0A",
          secondary: "#52525B",
          muted: "#A1A1AA",
        },
        primary: {
          DEFAULT: "#0A0A0A",
          foreground: "#FAFAFA",
          hover: "#27272A",
        },
        status: {
          healthy: {
            text: "#15803D",
            bg: "#F0FDF4",
            border: "#BBF7D0",
          },
          degraded: {
            text: "#B45309",
            bg: "#FFFBEB",
            border: "#FDE68A",
          },
          unhealthy: {
            text: "#B91C1C",
            bg: "#FEF2F2",
            border: "#FECACA",
          },
          draining: {
            text: "#52525B",
            bg: "#F4F4F5",
            border: "#E4E4E7",
          },
        },
      },
      borderRadius: {
        card: "8px",
        btn: "6px",
        badge: "9999px",
      },
      fontFamily: {
        sans: ["Inter", "-apple-system", "BlinkMacSystemFont", "Segoe UI", "Roboto", "sans-serif"],
        mono: ["JetBrains Mono", "SFMono-Regular", "Menlo", "Monaco", "Consolas", "monospace"],
      },
      boxShadow: {
        card: "0 1px 2px 0 rgba(0, 0, 0, 0.03)",
      },
    },
  },
  plugins: [],
};

export default config;

