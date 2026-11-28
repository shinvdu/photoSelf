<script setup>
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { API, api } from '../api'
import { showToast } from '../toast'
import { onImgError } from '../placeholder'
import AlbumDialog from './AlbumDialog.vue'

defineProps({
  albums: { type: Array, default: () => [] },
})

const emit = defineEmits(['open', 'created', 'deleted', 'error'])

const { t } = useI18n({ useScope: 'global' })

const showAlbumDialog = ref(false)

function openAlbumDialog() {
  showAlbumDialog.value = true
}

function closeAlbumDialog() {
  showAlbumDialog.value = false
}

function onCreated(name) {
  showToast(t('album.created', { name }))
  emit('created', name)
}

async function deleteAlbum(album) {
  if (!confirm(t('album.deleteConfirm', { name: album.name }))) return
  if (
    !confirm(
      t('album.deleteConfirmAgain', {
        name: album.name,
        count: album.item_count,
      }),
    )
  ) {
    return
  }
  try {
    await api(`${API}/albums/${album.id}`, { method: 'DELETE' })
    emit('deleted', album)
  } catch (err) {
    emit('error', err.message)
  }
}
</script>

<template>
  <section class="albums-view">
    <div class="view-head">
      <div>
        <h2>{{ t('nav.albums') }}</h2>
        <p>{{ t('album.hint') }}</p>
      </div>
      <button type="button" class="primary" @click="openAlbumDialog">
        ＋ {{ t('album.create') }}
      </button>
    </div>

    <div v-if="!albums.length" class="empty">
      <h2>{{ t('empty.noAlbumsTitle') }}</h2>
      <p>{{ t('empty.noAlbumsHint') }}</p>
    </div>
    <div class="album-grid">
      <article
        v-for="album in albums"
        :key="album.id"
        class="album-card"
        @click="emit('open', album)"
      >
        <img v-if="album.cover_url" :src="album.cover_url" :alt="album.name" @error="onImgError" />
        <div v-else class="album-cover-empty">▣</div>
        <div class="album-info">
          <strong>{{ album.name }}</strong>
          <small>{{ t('album.items', album.item_count) }}</small>
        </div>
        <button class="album-delete" :title="t('album.deleteTitle')" @click.stop="deleteAlbum(album)">×</button>
      </article>
    </div>

    <AlbumDialog
      v-if="showAlbumDialog"
      @created="onCreated"
      @close="closeAlbumDialog"
      @error="emit('error', $event)"
    />
  </section>
</template>