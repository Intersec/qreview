<script setup lang="ts">
import { ref } from 'vue';

const props = defineProps<{ start?: string; label: string; draft?: boolean }>();
const emit = defineEmits<{ save: [body: string, draft: boolean]; cancel: [] }>();

const body = ref(props.start ?? '');
const draft = ref(props.draft ?? false);

function save() {
  if (body.value.trim() === '') {
    return;
  }
  emit('save', body.value, draft.value);
}
</script>

<template>
  <form class="talk-box" @submit.prevent="save">
    <p class="talk-head">
      <span class="talk-who">New</span>
      <span>{{ label }}</span>
    </p>

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
      <label class="flex items-center gap-1">
        <input v-model="draft" type="checkbox" />
        draft
      </label>
      <span class="spacer"></span>
      <button type="button" class="action" @click="emit('cancel')">Cancel</button>
      <button type="submit" class="action" :disabled="body.trim() === ''">Save</button>
    </div>
    <p class="talk-hint">Markdown. Ctrl+Enter saves.</p>
  </form>
</template>
