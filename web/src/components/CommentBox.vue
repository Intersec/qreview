<script setup lang="ts">
import { ref } from 'vue';

const props = defineProps<{ start?: string; label: string }>();
const emit = defineEmits<{ save: [body: string]; cancel: [] }>();

const body = ref(props.start ?? '');

function save() {
  if (body.value.trim() === '') {
    return;
  }
  emit('save', body.value);
}
</script>

<template>
  <form class="talk-box" @submit.prevent="save">
    <p class="talk-head">{{ label }}</p>

    <div class="talk-body">
      <label class="sr-only" :for="`box-${label}`">{{ label }}</label>
      <textarea
        :id="`box-${label}`"
        v-model="body"
        class="talk-text"
        :placeholder="label"
        @keydown.ctrl.enter="save"
      ></textarea>
    </div>

    <div class="talk-foot">
      <span class="talk-hint">Markdown. Ctrl+Enter saves.</span>
      <span class="spacer"></span>
      <button type="button" class="action" @click="emit('cancel')">Cancel</button>
      <button type="submit" class="action" :disabled="body.trim() === ''">Save</button>
    </div>
  </form>
</template>
