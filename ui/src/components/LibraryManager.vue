<script setup>
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { API, api } from '../api'

const props = defineProps({
  scanning: { type: Boolean, default: false },
  error: { type: String, default: '' },
})

const emit = defineEmits(['scan-job', 'reload-info', 'reload-items', 'error', 'clear-error'])

const { t } = useI18n({ useScope: 'global' })

const libraryList = ref([])
const libraryLoading = ref(false)
const libraryMessage = ref('')
const showAddLibrary = ref(false)
const addingLibrary = ref(false)
const libraryError = ref('')
const newLibrary = ref({ path: '', name: '', read_only: true })
const browser = ref(null)
const browserLoading = ref(false)

async function loadManagedLibraries() {
  libraryLoading.value = true
  try {
    const data = await api(`${API}/libraries`)
    libraryList.value = data.libraries || []
  } catch (err) {
    emit('error', err.message)
  } finally {
    libraryLoading.value = false
  }
}

function openAddLibrary() {
  libraryError.value = ''
  newLibrary.value = { path: '', name: '', read_only: true }
  browser.value = null
  showAddLibrary.value = true
}

function closeAddLibrary() {
  if (addingLibrary.value) return
  showAddLibrary.value = false
  browser.value = null
}

async function browseTo(path = '') {
  browserLoading.value = true
  libraryError.value = ''
  try {
    const params = path ? `?path=${encodeURIComponent(path)}` : ''
    browser.value = await api(`${API}/fs/dirs${params}`)
  } catch (err) {
    libraryError.value = err.message
  } finally {
    browserLoading.value = false
  }
}

function chooseBrowserDir() {
  if (!browser.value?.path) return
  newLibrary.value.path = browser.value.path
  if (!newLibrary.value.name.trim()) {
    const parts = browser.value.path.split('/').filter(Boolean)
    newLibrary.value.name = parts[parts.length - 1] || ''
  }
  browser.value = null
}

async function addLibrary() {
  if (!newLibrary.value.path.trim() || addingLibrary.value) return
  addingLibrary.value = true
  libraryError.value = ''
  try {
    const result = await api(`${API}/libraries`, {
      method: 'POST',
      body: JSON.stringify({
        path: newLibrary.value.path.trim(),
        name: newLibrary.value.name.trim() || null,
        read_only: newLibrary.value.read_only,
      }),
    })
    libraryMessage.value = t('manage.added', { name: result.library?.name || '' })
    showAddLibrary.value = false
    browser.value = null
    await loadManagedLibraries()
    if (result.scan_job) emit('scan-job', result.scan_job)
  } catch (err) {
    libraryError.value = err.message
  } finally {
    addingLibrary.value = false
  }
}

async function scanLibraryEntry(library) {
  if (props.scanning) return
  libraryMessage.value = ''
  emit('clear-error')
  try {
    const data = await api(`${API}/libraries/${library.id}/scan`, { method: 'POST' })
    emit('scan-job', data.job_id)
  } catch (err) {
    emit('error', err.message)
  }
}

async function toggleLibrary(library) {
  libraryMessage.value = ''
  try {
    await api(`${API}/libraries/${library.id}`, {
      method: 'PATCH',
      body: JSON.stringify({ enabled: !library.enabled }),
    })
    await loadManagedLibraries()
  } catch (err) {
    emit('error', err.message)
  }
}

async function removeLibrary(library) {
  if (
    !confirm(
      t('manage.removeConfirm', { name: library.name, count: library.item_count }),
    )
  ) {
    return
  }
  libraryMessage.value = ''
  try {
    await api(`${API}/libraries/${library.id}`, { method: 'DELETE' })
    await loadManagedLibraries()
    emit('reload-info')
    emit('reload-items')
  } catch (err) {
    emit('error', err.message)
  }
}

async function importLibraries() {
  if (!confirm(t('manage.importConfirm'))) return
  libraryMessage.value = ''
  try {
    const data = await api(`${API}/libraries/import-config`, { method: 'POST' })
    libraryMessage.value = t('manage.importDone', { count: data.imported })
    await loadManagedLibraries()
    emit('reload-info')
  } catch (err) {
    emit('error', err.message)
  }
}

defineExpose({ loadManagedLibraries })

onMounted(loadManagedLibraries)
</script>

<template>
  <section class="manage-view">
    <div class="manage-head">
      <div>
        <h2>{{ t('manage.title') }}</h2>
        <p>{{ t('manage.hint') }}</p>
      </div>
      <div class="row manage-actions">
        <button type="button" @click="importLibraries">{{ t('manage.importConfig') }}</button>
        <button type="button" class="primary" @click="openAddLibrary">
          {{ t('manage.add') }}
        </button>
      </div>
    </div>

    <p v-if="libraryMessage" class="muted">{{ libraryMessage }}</p>
    <p v-if="error" class="error">{{ error }}</p>
    <p v-if="libraryLoading && !libraryList.length" class="muted">
      {{ t('actions.loading') }}
    </p>

    <div v-if="!libraryLoading && !libraryList.length" class="empty">
      <h2>{{ t('manage.empty') }}</h2>
      <p>{{ t('manage.emptyHint') }}</p>
    </div>

    <div class="library-list">
      <article
        v-for="library in libraryList"
        :key="library.id"
        class="library-card"
        :class="{ off: !library.enabled }"
      >
        <div class="library-main">
          <div class="library-title">
            <strong>{{ library.name }}</strong>
            <span v-if="library.read_only" class="badge">{{ t('manage.readOnly') }}</span>
            <span v-if="!library.enabled" class="badge warn">{{ t('manage.disabled') }}</span>
            <span class="badge source">{{ t('manage.source.' + library.source) }}</span>
          </div>
          <code class="library-path">{{ library.path }}</code>
          <small>{{ t('manage.items', library.item_count) }}</small>
        </div>
        <div class="row library-actions">
          <button
            type="button"
            :disabled="scanning || !library.enabled"
            @click="scanLibraryEntry(library)"
          >
            {{ t('manage.scan') }}
          </button>
          <button type="button" @click="toggleLibrary(library)">
            {{ library.enabled ? t('manage.disable') : t('manage.enable') }}
          </button>
          <button type="button" class="danger" @click="removeLibrary(library)">
            {{ t('manage.remove') }}
          </button>
        </div>
      </article>
    </div>

    <div v-if="showAddLibrary" class="manager-modal" @click.self="closeAddLibrary">
      <div class="manager-dialog">
        <h3>{{ t('manage.add') }}</h3>

        <label class="field">
          <span>{{ t('manage.path') }}</span>
          <div class="row">
            <input
              v-model="newLibrary.path"
              :placeholder="t('manage.pathPlaceholder')"
              spellcheck="false"
            />
            <button type="button" :disabled="browserLoading" @click="browseTo()">
              {{ t('manage.browse') }}
            </button>
          </div>
          <small>{{ t('manage.pathHint') }}</small>
        </label>

        <div v-if="browser" class="dir-browser">
          <div class="dir-head">
            <button
              v-if="browser.parent"
              type="button"
              @click="browseTo(browser.parent)"
            >
              ← {{ t('manage.up') }}
            </button>
            <code>{{ browser.path || t('manage.roots') }}</code>
            <button
              v-if="browser.path"
              type="button"
              class="primary"
              @click="chooseBrowserDir"
            >
              {{ t('manage.choose') }}
            </button>
          </div>
          <p v-if="browserLoading" class="muted">{{ t('actions.loading') }}</p>
          <ul v-else>
            <li v-for="entry in browser.entries" :key="entry.path">
              <button type="button" @click="browseTo(entry.path)">
                📁 {{ entry.name }}
              </button>
            </li>
          </ul>
          <p v-if="!browserLoading && !browser.entries?.length" class="muted">
            {{ t('manage.noSubdirs') }}
          </p>
        </div>

        <label class="field">
          <span>{{ t('manage.name') }}</span>
          <input
            v-model="newLibrary.name"
            :placeholder="t('manage.namePlaceholder')"
          />
        </label>

        <label class="check">
          <input v-model="newLibrary.read_only" type="checkbox" />
          {{ t('manage.readOnlyLabel') }}
        </label>

        <p v-if="libraryError" class="error">{{ libraryError }}</p>

        <div class="row dialog-actions">
          <button type="button" @click="closeAddLibrary">
            {{ t('actions.close') }}
          </button>
          <button
            type="button"
            class="primary"
            :disabled="addingLibrary || !newLibrary.path.trim()"
            @click="addLibrary"
          >
            {{ addingLibrary ? t('actions.loading') : t('manage.addConfirm') }}
          </button>
        </div>
      </div>
    </div>
  </section>
</template>