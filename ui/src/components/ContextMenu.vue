<script setup>
import { useI18n } from 'vue-i18n'

defineProps({
  menu: { type: Object, required: true },
  canDelete: { type: Boolean, default: false },
  count: { type: Number, default: 1 },
})

const emit = defineEmits(['view', 'download', 'album', 'delete'])

const { t } = useI18n({ useScope: 'global' })
</script>

<template>
  <div
    class="ctx-menu"
    :style="{ left: menu.x + 'px', top: menu.y + 'px' }"
    @click.stop
    @contextmenu.prevent
  >
    <p class="ctx-title">{{ menu.item.filename }}</p>
    <button type="button" @click="emit('view')">👁 {{ t('batch.view') }}</button>
    <button type="button" @click="emit('download')">⬇ {{ t('batch.download') }}</button>
    <button type="button" @click="emit('album')">＋ {{ t('batch.addToAlbum') }}</button>
    <button
      type="button"
      class="danger"
      :disabled="!canDelete"
      :title="canDelete ? '' : t('batch.readOnly')"
      @click="emit('delete')"
    >
      🗑 {{ t('batch.delete') }}
    </button>
    <p v-if="count > 1" class="ctx-hint">
      {{ t('batch.selected', { count }) }}
    </p>
  </div>
</template>