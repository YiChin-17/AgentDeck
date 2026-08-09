/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        background: 'var(--color-bg)',
        'bg-secondary': 'var(--color-bg-secondary)',
        surface: 'var(--color-surface)',
        'surface-hover': 'var(--color-surface-hover)',
        'surface-active': 'var(--color-surface-active)',
        border: 'var(--color-border)',
        'border-subtle': 'var(--color-border-subtle)',
        'border-faint': 'var(--color-border-faint)',
        accent: {
          DEFAULT: 'var(--color-accent)',
          light: 'var(--color-accent-light)',
          dark: 'var(--color-accent-dark)',
          bg: 'var(--color-accent-bg)',
          border: 'var(--color-accent-border)',
        },
        danger: {
          DEFAULT: 'var(--color-danger)',
          bg: 'var(--color-danger-bg)',
        },
        action: {
          DEFAULT: 'var(--color-action)',
          hover: 'var(--color-action-hover)',
          bg: 'var(--color-action-bg)',
          border: 'var(--color-action-border)',
        },
        lane: {
          library: 'var(--color-lane-library)',
          'library-bg': 'var(--color-lane-library-bg)',
          codex: 'var(--color-lane-codex)',
          'codex-bg': 'var(--color-lane-codex-bg)',
          claude: 'var(--color-lane-claude)',
          'claude-bg': 'var(--color-lane-claude-bg)',
          both: 'var(--color-lane-both)',
          'both-bg': 'var(--color-lane-both-bg)',
        },
      },
      boxShadow: {
        card: 'var(--shadow-card)',
        'card-hover': 'var(--shadow-card-hover)',
      },
      textColor: {
        primary: 'var(--color-text-primary)',
        secondary: 'var(--color-text-secondary)',
        tertiary: 'var(--color-text-tertiary)',
        muted: 'var(--color-text-muted)',
        faint: 'var(--color-text-faint)',
      },
      fontFamily: {
        sans: [
          '"SF Pro Text"',
          '"PingFang SC"',
          '"Hiragino Sans GB"',
          '"Noto Sans SC"',
          '"Microsoft YaHei"',
          '-apple-system',
          'BlinkMacSystemFont',
          '"Segoe UI"',
          'system-ui',
          'sans-serif',
        ],
        mono: [
          '"SF Mono"',
          '"Fira Code"',
          '"JetBrains Mono"',
          'Menlo',
          'Monaco',
          'monospace',
        ],
      },
    },
  },
  plugins: [],
}
