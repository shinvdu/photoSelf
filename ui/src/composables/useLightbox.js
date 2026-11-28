import { computed, onMounted, onUnmounted, ref } from 'vue'

// Lightbox zoom / pan / gesture state extracted from App.vue.
// `selected` and `currentItems` are refs owned by the caller.
export function useLightbox({ selected, currentItems }) {
  const zoomStage = ref(null)
  const zoom = ref(1)
  const panX = ref(0)
  const panY = ref(0)
  const dragging = ref(false)
  const hiResLoaded = ref(false)

  const metaOpen = ref(!compactLayout())

  const pointers = new Map()
  let layoutQueries = []
  let lastCompact = compactLayout()
  let dragState = null
  let pinchState = null
  let lastTap = { time: 0, x: 0, y: 0 }
  let suppressDblClickUntil = 0

  function compactLayout() {
    return (
      typeof window !== 'undefined' &&
      Boolean(window.matchMedia) &&
      (window.matchMedia('(max-width: 860px)').matches ||
        window.matchMedia('(orientation: portrait)').matches)
    )
  }

  const isZoomed = computed(() => zoom.value > 1.001)
  const zoomPercent = computed(() => `${Math.round(zoom.value * 100)}%`)
  const zoomStyle = computed(() => ({
    transform: `translate(${panX.value}px, ${panY.value}px) scale(${zoom.value})`,
    transition: dragging.value ? 'none' : 'transform 150ms ease',
  }))
  const showHiRes = computed(
    () => isZoomed.value && Boolean(selected.value?.media_url),
  )

  const selectedIndex = computed(() =>
    selected.value
      ? currentItems.value.findIndex((item) => item.id === selected.value.id)
      : -1,
  )

  function clampPan(x, y, scale = zoom.value) {
    const el = zoomStage.value
    if (!el) return { x: 0, y: 0 }
    const maxX = Math.max(0, (el.offsetWidth * (scale - 1)) / 2)
    const maxY = Math.max(0, (el.offsetHeight * (scale - 1)) / 2)
    return {
      x: Math.min(maxX, Math.max(-maxX, x)),
      y: Math.min(maxY, Math.max(-maxY, y)),
    }
  }

  function applyZoom(nextScale, clientX, clientY) {
    const scale = Math.min(5, Math.max(1, nextScale))
    if (scale === 1) {
      resetZoom()
      return
    }
    const el = zoomStage.value
    const rect = el?.getBoundingClientRect()
    const originX = clientX != null && rect ? clientX - (rect.left + rect.width / 2) : 0
    const originY = clientY != null && rect ? clientY - (rect.top + rect.height / 2) : 0
    const ratio = scale / zoom.value
    const next = clampPan(
      panX.value * ratio + originX * (1 - ratio),
      panY.value * ratio + originY * (1 - ratio),
      scale,
    )
    zoom.value = scale
    panX.value = next.x
    panY.value = next.y
  }

  function resetZoom() {
    zoom.value = 1
    panX.value = 0
    panY.value = 0
    hiResLoaded.value = false
  }

  function zoomBy(factor) {
    const rect = zoomStage.value?.getBoundingClientRect()
    applyZoom(
      zoom.value * factor,
      rect ? rect.left + rect.width / 2 : null,
      rect ? rect.top + rect.height / 2 : null,
    )
  }

  function onWheel(event) {
    if (selected.value?.kind === 'video') return
    applyZoom(zoom.value * Math.exp(-event.deltaY * 0.0015), event.clientX, event.clientY)
  }

  function onDoubleClick(event) {
    if (performance.now() < suppressDblClickUntil) return
    if (isZoomed.value) resetZoom()
    else applyZoom(2.5, event.clientX, event.clientY)
  }

  function onPointerDown(event) {
    if (event.pointerType === 'mouse' && event.button !== 0) return
    try {
      event.currentTarget.setPointerCapture?.(event.pointerId)
    } catch {
      // synthetic events may reference an unknown pointer id
    }
    pointers.set(event.pointerId, { x: event.clientX, y: event.clientY })
    if (pointers.size === 2) {
      const [a, b] = [...pointers.values()]
      pinchState = { distance: Math.hypot(a.x - b.x, a.y - b.y), zoom: zoom.value }
      dragState = null
      dragging.value = true
      return
    }
    if (pointers.size === 1) {
      dragState = {
        startX: event.clientX,
        startY: event.clientY,
        originX: panX.value,
        originY: panY.value,
        pointerType: event.pointerType,
        moved: false,
      }
      dragging.value = true
    }
  }

  function onPointerMove(event) {
    if (!pointers.has(event.pointerId)) return
    pointers.set(event.pointerId, { x: event.clientX, y: event.clientY })
    if (pointers.size >= 2 && pinchState) {
      const [a, b] = [...pointers.values()]
      const distance = Math.hypot(a.x - b.x, a.y - b.y)
      if (pinchState.distance > 0) {
        applyZoom(
          pinchState.zoom * (distance / pinchState.distance),
          (a.x + b.x) / 2,
          (a.y + b.y) / 2,
        )
      }
      return
    }
    if (!dragState) return
    const dx = event.clientX - dragState.startX
    const dy = event.clientY - dragState.startY
    if (Math.abs(dx) > 4 || Math.abs(dy) > 4) dragState.moved = true
    if (isZoomed.value) {
      const next = clampPan(dragState.originX + dx, dragState.originY + dy)
      panX.value = next.x
      panY.value = next.y
    }
  }

  function onPointerUp(event) {
    pointers.delete(event.pointerId)
    if (pointers.size < 2) pinchState = null
    if (pointers.size > 0) return
    if (dragState?.pointerType === 'touch' && !dragState.moved) {
      const now = performance.now()
      const dx = event.clientX - lastTap.x
      const dy = event.clientY - lastTap.y
      if (now - lastTap.time < 320 && Math.hypot(dx, dy) < 30) {
        lastTap = { time: 0, x: 0, y: 0 }
        suppressDblClickUntil = now + 500
        if (isZoomed.value) resetZoom()
        else applyZoom(2.5, event.clientX, event.clientY)
      } else {
        lastTap = { time: now, x: event.clientX, y: event.clientY }
      }
    } else if (dragState?.pointerType === 'touch') {
      lastTap = { time: 0, x: 0, y: 0 }
    }
    if (!isZoomed.value && dragState?.moved && dragState.pointerType === 'touch') {
      const dx = event.clientX - dragState.startX
      const dy = event.clientY - dragState.startY
      if (Math.abs(dx) > 60 && Math.abs(dx) > Math.abs(dy) * 1.4) {
        if (dx < 0) next()
        else prev()
      }
    }
    dragState = null
    dragging.value = false
  }

  function toggleMeta() {
    metaOpen.value = !metaOpen.value
  }

  function onLayoutChange() {
    const compact = compactLayout()
    if (compact !== lastCompact) {
      lastCompact = compact
      metaOpen.value = !compact
    }
  }

  function prev() {
    if (selectedIndex.value > 0) {
      selected.value = currentItems.value[selectedIndex.value - 1]
    }
  }

  function next() {
    if (
      selectedIndex.value >= 0 &&
      selectedIndex.value < currentItems.value.length - 1
    ) {
      selected.value = currentItems.value[selectedIndex.value + 1]
    }
  }

  onMounted(() => {
    layoutQueries = [
      window.matchMedia('(max-width: 860px)'),
      window.matchMedia('(orientation: portrait)'),
    ]
    for (const query of layoutQueries) query.addEventListener('change', onLayoutChange)
  })

  onUnmounted(() => {
    for (const query of layoutQueries) query.removeEventListener('change', onLayoutChange)
  })

  return {
    zoomStage,
    zoom,
    panX,
    panY,
    dragging,
    hiResLoaded,
    metaOpen,
    isZoomed,
    zoomPercent,
    zoomStyle,
    showHiRes,
    selectedIndex,
    resetZoom,
    zoomBy,
    onWheel,
    onDoubleClick,
    onPointerDown,
    onPointerMove,
    onPointerUp,
    toggleMeta,
    prev,
    next,
  }
}