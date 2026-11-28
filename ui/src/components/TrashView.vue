<script setup>
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { API, api } from '../api'
import { formatDate } from '../format'

const emit = defineEmits(['restored', 'error'])

const { t } = useI18n({ useScope: 'global' })

const trashEntries = ref([])

async function loadTrash() {
  try {
    const data = await api(`${API}/trash`)
    trashEntries.value = data.entries || []
  } catch (err) {
    emit('error', err.message)
  }
}

async function restoreEntry(entry) {
  try {
    await api(`${API}/trash/${entry.id}/restore`, { method: 'POST' })
    await loadTrash()
    emit('restored')
  } catch (err) {
    emit('error', err.message)
  }
}

async function purgeEntry(entry) {
  if (!confirm(t('trash.purgeConfirm', { name: entry.filename }))) return
  try {
    await api(`${API}/trash/${entry.id}`, { method: 'DELETE' })
    await loadTrash()
  } catch (err) {
    emit('error', err.message)
  }
}

onMounted(loadTrash)
</script>

<template>
  <section class="trash-view">
    <h2>{{ t('trash.title') }}</h2>
    <p v-if="!trashEntries.length" class="empty">{{ t('empty.trash') }}</p>
    <div v-for="entry in trashEntries" :key="entry.id" class="trash-row">
      <div>
        <strong>{{ entry.filename }}</strong>
        <small>{{ t('trash.deletedAt', { date: formatDate(entry.deleted_at, t), library: entry.library_id }) }}</small>
      </div>
      <div class="row">
        <button @click="restoreEntry(entry)">{{ t('trash.restore') }}</button>
        <button class="danger" @click="purgeEntry(entry)">{{ t('trash.purge') }}</button>
      </div>
    </div>
  </section>
</template>