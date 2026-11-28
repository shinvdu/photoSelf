<script setup>
import { useI18n } from 'vue-i18n'

defineProps({
  view: { type: String, default: 'timeline' },
  info: { type: Object, default: null },
  tags: { type: Array, default: () => [] },
  writableLibraries: { type: Array, default: () => [] },
  favoritesOnly: { type: Boolean, default: false },
  selectionMode: { type: Boolean, default: false },
  uploading: { type: Boolean, default: false },
  scanning: { type: Boolean, default: false },
  theme: { type: String, default: 'light' },
})

const query = defineModel('query', { type: String, default: '' })
const tagFilter = defineModel('tagFilter', { type: String, default: '' })
const uploadLibrary = defineModel('uploadLibrary', { type: [String, Number], default: '' })
const currentLocale = defineModel('currentLocale', { type: String, default: 'zh-CN' })

const emit = defineEmits([
  'search',
  'tag-change',
  'toggle-favorites',
  'toggle-selection',
  'upload',
  'scan',
  'toggle-theme',
  'language-change',
  'navigate',
])

const { t } = useI18n({ useScope: 'global' })
</script>

<template>
  <header class="topbar">
    <div class="brand">
      <span class="logo">❀</span>
      <span>photoSelf</span>
    </div>

    <nav class="tabs">
      <button :class="{ active: view === 'timeline' }" @click="emit('navigate', 'timeline')">
        {{ t('nav.timeline') }}
      </button>
      <button :class="{ active: view === 'albums' || view === 'album' }" @click="emit('navigate', 'albums')">
        {{ t('nav.albums') }}
      </button>
      <button
        v-if="info && info.write_enabled && info.trash_enabled"
        :class="{ active: view === 'trash' }"
        @click="emit('navigate', 'trash')"
      >
        {{ t('nav.trash') }}
      </button>
      <button
        v-if="info && info.manage_enabled"
        :class="{ active: view === 'manage' }"
        @click="emit('navigate', 'manage')"
      >
        {{ t('manage.title') }}
      </button>
    </nav>

    <form class="search" @submit.prevent="emit('search')">
      <input v-model="query" type="search" :placeholder="t('search.placeholder')" />
      <button type="submit">{{ t('search.submit') }}</button>
    </form>

    <div class="actions">
      <select v-if="view === 'timeline'" v-model="tagFilter" @change="emit('tag-change')">
        <option value="">{{ t('filters.allTags') }}</option>
        <option v-for="tag in tags" :key="tag.id" :value="tag.name">
          {{ tag.name }}
        </option>
      </select>
      <select
        v-if="writableLibraries.length > 1"
        v-model="uploadLibrary"
        :title="t('actions.upload')"
      >
        <option v-for="library in writableLibraries" :key="library.id" :value="library.id">
          {{ library.name || library.id }}
        </option>
      </select>
      <button :class="{ active: favoritesOnly }" @click="emit('toggle-favorites')">
        {{ t('filters.favorites') }}
      </button>
      <button
        v-if="view === 'timeline' || view === 'album'"
        :class="{ active: selectionMode }"
        @click="emit('toggle-selection')"
      >
        {{ selectionMode ? t('batch.cancel') : t('batch.select') }}
      </button>
      <button
        v-if="info && info.write_enabled && writableLibraries.length"
        :disabled="uploading"
        @click="emit('upload')"
      >
        {{ uploading ? t('actions.uploading') : t('actions.upload') }}
      </button>
      <button :disabled="scanning" @click="emit('scan')">
        {{ scanning ? t('actions.scanning') : t('actions.scan') }}
      </button>
    </div>

    <div class="topbar-tools">
      <button
        type="button"
        class="theme-toggle"
        :title="theme === 'dark' ? t('theme.toLight') : t('theme.toDark')"
        :aria-label="theme === 'dark' ? t('theme.toLight') : t('theme.toDark')"
        @click="emit('toggle-theme')"
      >
        {{ theme === 'dark' ? '☀️' : '🌙' }}
      </button>
      <select v-model="currentLocale" :title="t('language.label')" @change="emit('language-change', $event)">
        <option value="zh-CN">中文</option>
        <option value="en-US">English</option>
      </select>
    </div>
  </header>
</template>