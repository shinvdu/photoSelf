import { computed, ref } from 'vue'

// Selection mode, multi-select and context-menu state for the tile grid.
// `currentItems` is a ref owned by the caller; `openItem` opens the lightbox.
export function useSelection({ currentItems, openItem }) {
  const selectionMode = ref(false)
  const selectedIds = ref([])
  const contextMenu = ref(null)
  const showAlbumPicker = ref(false)
  const batchBusy = ref(false)

  let lastSelectedId = null
  let tilePressTimer = null
  let suppressTileClick = false

  const selectionActive = computed(
    () => selectionMode.value || selectedIds.value.length > 0,
  )

  const selectedItems = computed(() =>
    currentItems.value.filter((item) => selectedIds.value.includes(item.id)),
  )

  function isSelected(id) {
    return selectedIds.value.includes(id)
  }

  function clearSelection() {
    selectedIds.value = []
    selectionMode.value = false
    contextMenu.value = null
    showAlbumPicker.value = false
    lastSelectedId = null
  }

  function toggleSelectionMode() {
    if (selectionMode.value) clearSelection()
    else {
      selectionMode.value = true
      selectedIds.value = []
    }
  }

  function toggleSelect(item) {
    const id = item.id
    selectedIds.value = isSelected(id)
      ? selectedIds.value.filter((value) => value !== id)
      : [...selectedIds.value, id]
    lastSelectedId = id
  }

  function handleTileClick(item, event) {
    if (suppressTileClick) {
      suppressTileClick = false
      return
    }
    if (event.shiftKey && lastSelectedId) {
      const list = currentItems.value
      const from = list.findIndex((entry) => entry.id === lastSelectedId)
      const to = list.findIndex((entry) => entry.id === item.id)
      if (from >= 0 && to >= 0) {
        const [start, end] = from < to ? [from, to] : [to, from]
        const ids = new Set(selectedIds.value)
        for (let index = start; index <= end; index += 1) ids.add(list[index].id)
        selectedIds.value = [...ids]
        selectionMode.value = true
        return
      }
    }
    if (event.metaKey || event.ctrlKey || selectionMode.value) {
      selectionMode.value = true
      toggleSelect(item)
      return
    }
    openItem(item)
  }

  function openContextMenu(event, item) {
    if (!isSelected(item.id)) {
      selectedIds.value = [item.id]
      selectionMode.value = true
    }
    lastSelectedId = item.id
    const width = 216
    const height = 224
    contextMenu.value = {
      item,
      x: Math.max(8, Math.min(event.clientX, window.innerWidth - width - 8)),
      y: Math.max(8, Math.min(event.clientY, window.innerHeight - height - 8)),
    }
  }

  function closeContextMenu() {
    contextMenu.value = null
  }

  function onTilePointerDown(event, item) {
    if (event.pointerType !== 'touch') return
    tilePressTimer = setTimeout(() => {
      tilePressTimer = null
      suppressTileClick = true
      setTimeout(() => {
        suppressTileClick = false
      }, 800)
      if (!isSelected(item.id)) {
        selectedIds.value = [item.id]
        selectionMode.value = true
      }
      openContextMenu(event, item)
    }, 550)
  }

  function onTilePointerUp() {
    if (tilePressTimer) {
      clearTimeout(tilePressTimer)
      tilePressTimer = null
    }
  }

  return {
    selectionMode,
    selectedIds,
    contextMenu,
    showAlbumPicker,
    batchBusy,
    selectionActive,
    selectedItems,
    isSelected,
    clearSelection,
    toggleSelectionMode,
    toggleSelect,
    handleTileClick,
    openContextMenu,
    closeContextMenu,
    onTilePointerDown,
    onTilePointerUp,
  }
}