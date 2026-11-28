<script setup>
import { useI18n } from 'vue-i18n'

defineProps({
  count: { type: Number, default: 0 },
  canDelete: { type: Boolean, default: false },
  showDelete: { type: Boolean, default: false },
  busy: { type: Boolean, default: false },
})

const emit = defineEmits(['view', 'download', 'add-album', 'delete', 'clear'])

const { t } = useI18n({ useScope: 'global' })
</script>

<template>
  <div class="selection-bar">
    <span class="selection-count">
      {{ t('batch.selected', { count }) }}
    </span>
    <button type="button" :disabled="!count" @click="emit('view')">
      {{ t('batch.view') }}
    </button>
    <button type="button" :disabled="!count" @click="emit('download')">
      {{ t('batch.download') }}
    </button>
    <button type="button" :disabled="!count" @click="emit('add-album')">
      {{ t('batch.addToAlbum') }}
    </button>
    <button
      v-if="showDelete"
      type="button"
      class="danger"
      :disabled="!canDelete || busy"
      @click="emit('delete')"
    >
      {{ t('batch.delete') }}
    </button>
    <button type="button" @click="emit('clear')">{{ t('batch.clear') }}</button>
  </div>
</template>