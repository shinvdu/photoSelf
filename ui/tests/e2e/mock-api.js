// Shared mock backend for the Playwright interaction tests. No real
// router/API is involved: every /api/v1 request is fulfilled here.
const PIXEL =
  'data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7'

const TAGS = [
  { id: 1, name: '风景' },
  { id: 2, name: '家人' },
]

function mediaItem(id, overrides = {}) {
  const day = id <= 4 ? '2026-11-20' : '2026-11-21'
  return {
    id,
    filename: `photo-${id}.jpg`,
    kind: 'image',
    taken_at: `${day}T10:0${id}:00`,
    library_id: 'lib1',
    thumb_url: PIXEL,
    media_url: PIXEL,
    preview_url: PIXEL,
    favorite: false,
    tags: [],
    width: 1600,
    height: 1200,
    size: 1024 * id,
    ...overrides,
  }
}

export async function mockApi(page) {
  const state = {
    items: Array.from({ length: 8 }, (_, index) => {
      const id = index + 1
      if (id === 3) return mediaItem(id, { favorite: true })
      if (id === 4) return mediaItem(id, { tags: [{ id: 1, name: '风景' }] })
      if (id === 5) return mediaItem(id, { tags: [{ id: 1, name: '风景' }] })
      return mediaItem(id)
    }),
    albums: [
      { id: 1, name: '旅行', item_count: 3, cover_url: PIXEL },
      { id: 2, name: '家庭', item_count: 2, cover_url: null },
    ],
    albumItems: { 1: [1, 2, 3], 2: [4, 5] },
    libraries: [
      {
        id: 'lib1',
        name: 'Photos',
        path: '/photos',
        item_count: 8,
        enabled: true,
        read_only: false,
        source: 'config',
      },
    ],
    trash: [
      {
        id: 't1',
        filename: 'old.jpg',
        deleted_at: '2026-11-01T09:00:00',
        library_id: 'lib1',
      },
    ],
  }

  const info = () => ({
    version: '0.7.3',
    item_count: state.items.length,
    write_enabled: true,
    trash_enabled: true,
    manage_enabled: true,
    transcode_enabled: false,
    libraries: state.libraries,
  })

  await page.route('**/api/v1/**', async (route) => {
    const request = route.request()
    const url = new URL(request.url())
    const path = url.pathname.replace(/^\/api\/v1/, '')
    const method = request.method()
    const respond = (body, status = 200) =>
      route.fulfill({
        status,
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(body),
      })
    const noContent = () => route.fulfill({ status: 204, body: '' })

    const albumMatch = path.match(/^\/albums\/(\d+)\/items$/)
    const albumItemMatch = path.match(/^\/albums\/(\d+)\/items\/(\d+)$/)
    const albumMatchDelete = path.match(/^\/albums\/(\d+)$/)
    const libraryScanMatch = path.match(/^\/libraries\/([^/]+)\/scan$/)
    const libraryMatch = path.match(/^\/libraries\/([^/]+)$/)
    const itemMatch = path.match(/^\/items\/(\d+)$/)
    const trashRestoreMatch = path.match(/^\/trash\/([^/]+)\/restore$/)
    const trashMatch = path.match(/^\/trash\/([^/]+)$/)

    if (method === 'GET' && path === '/info') return respond(info())
    if (method === 'GET' && path === '/tags') return respond({ tags: TAGS })
    if (method === 'GET' && path === '/items') {
      let list = [...state.items]
      const q = url.searchParams.get('q')
      const tag = url.searchParams.get('tag')
      const favorite = url.searchParams.get('favorite')
      if (q) list = list.filter((item) => item.filename.includes(q))
      if (tag) list = list.filter((item) => item.tags.some((t) => t.name === tag))
      if (favorite === 'true') list = list.filter((item) => item.favorite)
      return respond({ items: list, next_cursor: null })
    }
    if (method === 'GET' && path === '/albums') return respond({ albums: state.albums })
    if (method === 'POST' && path === '/albums') {
      const body = JSON.parse(request.postData() || '{}')
      const album = {
        id: state.albums.length + 1,
        name: body.name,
        item_count: 0,
        cover_url: null,
      }
      state.albums.push(album)
      state.albumItems[album.id] = []
      return respond({ album })
    }
    if (method === 'GET' && albumMatch) {
      const albumId = albumMatch[1]
      const ids = state.albumItems[albumId] || []
      return respond({
        items: ids
          .map((id) => state.items.find((item) => item.id === id))
          .filter(Boolean),
      })
    }
    if (method === 'POST' && albumMatch) {
      const albumId = albumMatch[1]
      const body = JSON.parse(request.postData() || '{}')
      const ids = state.albumItems[albumId] || (state.albumItems[albumId] = [])
      if (!ids.includes(body.item_id)) ids.push(body.item_id)
      return respond({ ok: true })
    }
    if (method === 'DELETE' && albumItemMatch) {
      const albumId = albumItemMatch[1]
      const itemId = Number(albumItemMatch[2])
      state.albumItems[albumId] = (state.albumItems[albumId] || []).filter(
        (id) => id !== itemId,
      )
      return noContent()
    }
    if (method === 'DELETE' && albumMatchDelete) {
      const albumId = Number(albumMatchDelete[1])
      state.albums = state.albums.filter((album) => album.id !== albumId)
      delete state.albumItems[albumId]
      return noContent()
    }
    if (method === 'GET' && path === '/trash') return respond({ entries: state.trash })
    if (method === 'POST' && trashRestoreMatch) {
      state.trash = state.trash.filter((entry) => entry.id !== trashRestoreMatch[1])
      return noContent()
    }
    if (method === 'POST' && trashMatch) {
      state.trash = state.trash.filter((entry) => entry.id !== trashMatch[1])
      return noContent()
    }
    if (method === 'DELETE' && trashMatch) {
      state.trash = state.trash.filter((entry) => entry.id !== trashMatch[1])
      return noContent()
    }
    if (method === 'GET' && path === '/libraries') {
      return respond({ libraries: state.libraries })
    }
    if (method === 'GET' && path === '/fs/dirs') {
      return respond({
        path: url.searchParams.get('path') || '/',
        parent: null,
        entries: [{ name: 'photos', path: '/photos' }],
      })
    }
    if (method === 'POST' && path === '/libraries') {
      const body = JSON.parse(request.postData() || '{}')
      const library = {
        id: `lib${state.libraries.length + 1}`,
        name: body.name || body.path.split('/').filter(Boolean).pop(),
        path: body.path,
        item_count: 0,
        enabled: true,
        read_only: Boolean(body.read_only),
        source: 'ui',
      }
      state.libraries.push(library)
      return respond({ library, scan_job: 'job1' })
    }
    if (method === 'POST' && path === '/libraries/import-config') {
      return respond({ imported: state.libraries.length })
    }
    if (method === 'POST' && libraryScanMatch) return respond({ job_id: 'job1' })
    if (method === 'PATCH' && libraryMatch) {
      const library = state.libraries.find((entry) => entry.id === libraryMatch[1])
      if (library) {
        const body = JSON.parse(request.postData() || '{}')
        Object.assign(library, body)
      }
      return respond({ library })
    }
    if (method === 'DELETE' && libraryMatch) {
      state.libraries = state.libraries.filter((entry) => entry.id !== libraryMatch[1])
      return noContent()
    }
    if (method === 'POST' && path === '/upload') {
      const item = mediaItem(state.items.length + 1, {
        filename: 'new-upload.png',
        taken_at: '2026-11-22T10:00:00',
      })
      state.items.push(item)
      return respond(item)
    }
    if (method === 'POST' && path === '/scan') return respond({ job_id: 'job1' })
    if (method === 'GET' && path.startsWith('/jobs/')) {
      return respond({ state: 'completed', progress: state.items.length, total: state.items.length })
    }
    if (method === 'PATCH' && itemMatch) {
      const item = state.items.find((entry) => entry.id === Number(itemMatch[1]))
      const body = JSON.parse(request.postData() || '{}')
      if (item) Object.assign(item, body)
      return respond(item)
    }
    if (method === 'DELETE' && itemMatch) {
      state.items = state.items.filter((entry) => entry.id !== Number(itemMatch[1]))
      return noContent()
    }
    if (method === 'GET' && itemMatch) {
      return respond(state.items.find((entry) => entry.id === Number(itemMatch[1])))
    }
    if (method === 'POST' && /^\/items\/\d+\/tags$/.test(path)) {
      return respond(state.items[0])
    }
    if (method === 'DELETE' && /^\/items\/\d+\/tags\/\d+$/.test(path)) {
      return respond(state.items[0])
    }

    return route.fulfill({
      status: 404,
      headers: { 'content-type': 'application/problem+json' },
      body: JSON.stringify({ title: 'not found', detail: `unmocked ${method} ${path}` }),
    })
  })

  return state
}

export const UPLOAD_BUFFER = Buffer.from(PIXEL.split(',')[1], 'base64')