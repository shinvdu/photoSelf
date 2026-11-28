import { test, expect } from '@playwright/test'
import { mockApi, UPLOAD_BUFFER } from './mock-api.js'

async function openTimeline(page) {
  await page.goto('/#/timeline')
  await expect(page.locator('.tile').first()).toBeVisible()
}

async function openFirstLightbox(page) {
  await page.locator('.tile').first().click()
  await expect(page.locator('.lightbox')).toBeVisible()
}

test('timeline loads and the lightbox opens and closes with real clicks', async ({ page }) => {
  await mockApi(page)
  await openTimeline(page)
  await expect(page.locator('.tile')).toHaveCount(8)

  await openFirstLightbox(page)
  await expect(page.locator('.zoom-value')).toHaveText('100%')

  await page.locator('.lightbox .close').click()
  await expect(page.locator('.lightbox')).toBeHidden()
})

test('lightbox zoom reaches 250%, resets, and Escape closes it', async ({ page }) => {
  await mockApi(page)
  await openTimeline(page)
  await openFirstLightbox(page)

  await page.locator('.zoom-stage').dblclick()
  await expect(page.locator('.zoom-value')).toHaveText('250%')

  await page.locator('.zoom-value').click()
  await expect(page.locator('.zoom-value')).toHaveText('100%')

  await page.keyboard.press('Escape')
  await expect(page.locator('.lightbox')).toBeHidden()
})

test('albums page creates a new album through the dialog', async ({ page }) => {
  const state = await mockApi(page)
  await page.goto('/#/albums')
  await expect(page.locator('.albums-view')).toBeVisible()

  await page.locator('.view-head button.primary').click()
  await expect(page.locator('.album-dialog')).toBeVisible()

  await page.locator('.album-dialog input').fill('测试相册')
  await page.locator('.album-dialog .primary').click()

  await expect(page.locator('.toast')).toContainText('已创建相册「测试相册」')
  await expect(page.locator('.album-dialog')).toBeHidden()
  await expect(page.locator('.album-card')).toHaveCount(state.albums.length)
})

test('multi-select shows the batch bar and clears the selection', async ({ page }) => {
  await mockApi(page)
  await openTimeline(page)

  await page.getByRole('button', { name: '选择', exact: true }).click()
  await page.locator('.tile').nth(0).click()
  await page.locator('.tile').nth(1).click()

  await expect(page.locator('.selection-bar')).toBeVisible()
  await expect(page.locator('.selection-count')).toHaveText('已选 2 项')

  await page.locator('.selection-bar').getByRole('button', { name: '取消', exact: true }).click()
  await expect(page.locator('.selection-bar')).toBeHidden()
})

test('right click opens the four-item context menu', async ({ page }) => {
  await mockApi(page)
  await openTimeline(page)

  await page.locator('.tile').first().click({ button: 'right' })
  const menu = page.locator('.ctx-menu')
  await expect(menu).toBeVisible()
  await expect(menu.locator('> button')).toHaveCount(4)
  await expect(menu).toContainText('查看')
  await expect(menu).toContainText('删除')
})

test('deep links, refresh and browser back keep the routed view', async ({ page }) => {
  await mockApi(page)

  await page.goto('/#/albums')
  await expect(page.locator('.albums-view')).toBeVisible()
  await page.reload()
  await expect(page.locator('.albums-view')).toBeVisible()

  await page.goto('/#/albums/1')
  await expect(page.locator('.album-header')).toContainText('旅行')
  await page.reload()
  await expect(page.locator('.album-header')).toContainText('旅行')
  await expect(page.locator('.album-header')).toHaveClass(/album-header/)

  await page.goBack()
  await expect(page).toHaveURL(/#\/albums$/)
  await expect(page.locator('.albums-view')).toBeVisible()

  await page.goto('/#/not-a-route')
  await expect(page).toHaveURL(/#\/timeline$/)
  await expect(page.locator('.tile').first()).toBeVisible()
})

test('timeline query string drives the favorite filter on a deep link', async ({ page }) => {
  await mockApi(page)
  await page.goto('/#/timeline?favorite=true')
  await expect(page.locator('.tile')).toHaveCount(1)
  await expect(page.locator('.tile').first()).toBeVisible()
})

test('uploading a file shows the upload toast and refreshes the timeline', async ({ page }) => {
  const state = await mockApi(page)
  await openTimeline(page)
  await expect(page.locator('.tile')).toHaveCount(8)

  await page.locator('input.hidden-input').setInputFiles({
    name: 'new-upload.png',
    mimeType: 'image/png',
    buffer: UPLOAD_BUFFER,
  })

  await expect(page.locator('.toast')).toContainText('已上传 1 个文件')
  await expect(page.locator('.tile')).toHaveCount(state.items.length)
})

test('trash and libraries views load their data directly from the URL', async ({ page }) => {
  await mockApi(page)

  await page.goto('/#/trash')
  await expect(page.locator('.trash-row')).toContainText('old.jpg')
  await page.getByRole('button', { name: '恢复', exact: true }).click()
  await expect(page.locator('.trash-row')).toHaveCount(0)
  await expect(page.locator('.trash-view')).toContainText('回收站是空的')

  await page.goto('/#/libraries')
  await expect(page.locator('.library-card')).toContainText('Photos')
  await page.getByRole('button', { name: '添加媒体库', exact: true }).click()
  await expect(page.locator('.manager-dialog')).toBeVisible()
  await expect(page.locator('.manager-dialog input').first()).toBeVisible()
})

test('theme toggle and portrait lightbox drawer do not regress', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 })
  await mockApi(page)
  await openTimeline(page)

  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light')
  await page.getByRole('button', { name: '切换到深色模式' }).click()
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark')

  await openFirstLightbox(page)
  await expect(page.locator('.meta-handle')).toBeVisible()
  await expect(page.locator('.lightbox .meta')).not.toHaveClass(/open/)
  // In portrait the drawer handle replaces the landscape-only ⓘ toggle.
  await expect(page.locator('.info-toggle')).toBeHidden()
  // Prev/next navigation must remain visible in portrait as well.
  await expect(page.locator('.lightbox .nav.prev')).toBeVisible()
  await expect(page.locator('.lightbox .nav.next')).toBeVisible()
  const before = await page.locator('.meta-handle .meta-title').innerText()
  await page.locator('.lightbox .nav.next').click()
  await expect(page.locator('.meta-handle .meta-title')).not.toHaveText(before)
})

test('album deletion requires two confirmations and removes the card', async ({ page }) => {
  const state = await mockApi(page)
  const initialCount = state.albums.length
  await page.goto('/#/albums')
  await expect(page.locator('.album-card')).toHaveCount(initialCount)

  const dialogs = []
  page.on('dialog', async (dialog) => {
    dialogs.push(dialog.message())
    await dialog.accept()
  })

  const card = page.locator('.album-card').first()
  await card.hover()
  await card.locator('.album-delete').click()

  await expect(page.locator('.album-card')).toHaveCount(initialCount - 1)
  expect(dialogs).toHaveLength(2)
  expect(dialogs[0]).toContain('旅行')
})

test('uploading inside an album adds the file to that album', async ({ page }) => {
  const state = await mockApi(page)
  await page.goto('/#/albums/1')
  await expect(page.locator('.album-header')).toContainText('旅行')
  await expect(page.locator('.tile')).toHaveCount(3)

  await page.locator('input.hidden-input').setInputFiles({
    name: 'album-upload.png',
    mimeType: 'image/png',
    buffer: UPLOAD_BUFFER,
  })

  await expect(page.locator('.toast')).toContainText('已上传并加入相册「旅行」：1 个文件')
  await expect(page.locator('.tile')).toHaveCount(4)
  expect(state.albumItems[1]).toContain(9)
})
