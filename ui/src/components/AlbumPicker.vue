<script setup>
import { useI18n } from 'vue-i18n'

defineProps({
  albums: { type: Array, default: () => [] },
  count: { type: Number, default: 0 },
  busy: { type: Boolean, default: false },
})

const emit = defineEmits(['pick', 'close'])

const { t } = useI18n({ useScope: 'global' })
</script>

<template>
  <div class="manager-modal" @click.self="emit('close')">
    <div class="manager-dialog album-picker">
      <h3>{{ t('batch.addToAlbum') }}</h3>
      <p class="muted">
        {{ t('batch.selected', { count }) }}
      </p>
      <div v-if="albums.length" class="album-picker-list">
        <button
          v-for="album in albums"
          :key="album.id"
          type="button"
          :disabled="busy"
          @click="emit('pick', album)"
        >
          <strong>{{ album.name }}</strong>
          <small>{{ t('manage.items', album.item_count) }}</small>
        </button>
      </div>
      <p v-else class="muted">{{ t('batch.noAlbums') }}</p>
      <div class="dialog-actions">
        <button type="button" @click="emit('close')">{{ t('actions.close') }}</button>
      </div>
    </div>
  </div>
</template>