import { ref } from 'vue'

const THEME_COLORS = { light: '#fff8fb', dark: '#171118' }

export function useTheme() {
  const theme = ref(document.documentElement.dataset.theme || 'light')

  function applyTheme(value) {
    theme.value = value
    document.documentElement.dataset.theme = value
    try {
      localStorage.setItem('photoself-theme', value)
    } catch {
      // private mode / disabled storage
    }
    const meta = document.querySelector('meta[name="theme-color"]')
    if (meta) meta.setAttribute('content', THEME_COLORS[value])
  }

  function toggleTheme() {
    applyTheme(theme.value === 'dark' ? 'light' : 'dark')
  }

  function applyThemeMeta() {
    const meta = document.querySelector('meta[name="theme-color"]')
    if (meta) meta.setAttribute('content', THEME_COLORS[theme.value])
  }

  return { theme, THEME_COLORS, toggleTheme, applyThemeMeta }
}