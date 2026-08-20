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
  <form class="rounded border border-slate-300 p-2 dark:border-slate-600" @submit.prevent="save">
    <label class="sr-only" :for="`box-${label}`">{{ label }}</label>
    <textarea
      :id="`box-${label}`"
      v-model="body"
      rows="3"
      class="w-full rounded border border-slate-300 p-1 font-sans text-sm dark:border-slate-600 dark:bg-slate-800"
      :placeholder="label"
      @keydown.ctrl.enter="save"
    ></textarea>

    <div class="mt-1 flex items-center gap-2 text-xs">
      <button
        type="submit"
        class="rounded bg-slate-800 px-2 py-1 text-white disabled:opacity-50 dark:bg-slate-200 dark:text-slate-900"
        :disabled="body.trim() === ''"
      >
        Save
      </button>
      <button
        type="button"
        class="rounded border border-slate-300 px-2 py-1 dark:border-slate-600"
        @click="emit('cancel')"
      >
        Cancel
      </button>
      <label class="ml-auto flex items-center gap-1">
        <input v-model="draft" type="checkbox" />
        draft
      </label>
    </div>
    <p class="mt-1 text-xs text-slate-500 dark:text-slate-400">Markdown. Ctrl+Enter saves.</p>
  </form>
</template>
