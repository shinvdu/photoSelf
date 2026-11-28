<script setup>
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { API, api } from '../api'

const emit = defineEmits(['close', 'created', 'error'])

const { t } = useI18n({ useScope: 'global' })

const newAlbumName = ref('')
const albumError = ref('')
const submitting = ref(false)

function close() {
  if (submitting.value) return
  emit('close')
}

async function createAlbum() {
  const name = newAlbumName.value.trim()
  if (!name || submitting.value) return
  submitting.value = true
  albumError.value = ''
  try {
    await api(`${API}/albums`, {
      method: 'POST',
      body: JSON.stringify({ name }),
    })
    newAlbumName.value = ''
    emit('created', name)
    emit('close')
  } catch (err) {
    albumError.value = err.message
    emit('error', err.message)
  } finally {
    submitting.value = false
  }
}
</script>

<template>
  <div class="manager-modal" @click.self="close">
    <div class="manager-dialog album-dialog">
      <h3>{{ t('album.createTitle') }}</h3>
      <label class="field">
        <span>{{ t('album.nameLabel') }}</span>
        <input
          v-model="newAlbumName"
          :placeholder="t('album.createPlaceholder')"
          @keyup.enter="createAlbum"
        />
      </label>
      <p v-if="albumError" class="error">{{ albumError }}</p>
      <div class="dialog-actions">
        <button type="button" @click="close">
          {{ t('actions.close') }}
        </button>
        <button
          type="button"
          class="primary"
          :disabled="!newAlbumName.trim() || submitting"
          @click="createAlbum"
        >
          {{ t('album.create') }}
        </button>
      </div>
    </div>
  </div>
</template>