export function formatDate(value, t) {
  return value ? value.replace('T', ' ') : t('date.undated')
}

export function formatDuration(ms) {
  if (!ms) return ''
  const total = Math.round(ms / 1000)
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`
}

export function groupByDay(list) {
  const map = new Map()
  for (const item of list) {
    const day = (item.taken_at || '1970-01-01').slice(0, 10)
    if (!map.has(day)) map.set(day, [])
    map.get(day).push(item)
  }
  return [...map.entries()].map(([date, items]) => ({ date, items }))
}

export function dateLabel(day, locale, t) {
  if (!day || day.startsWith('1970-')) return t('date.undated')
  const date = new Date(`${day}T00:00:00`)
  return date.toLocaleDateString(locale, {
    year: 'numeric',
    month: 'long',
    day: 'numeric',
    weekday: 'short',
  })
}

export function formatBytes(bytes) {
  if (!bytes) return ''
  const units = ['B', 'KB', 'MB', 'GB']
  let index = 0
  let value = bytes
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024
    index += 1
  }
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${units[index]}`
}