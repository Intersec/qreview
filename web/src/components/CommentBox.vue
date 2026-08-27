<script setup lang="ts">
import { onMounted, ref, watch } from 'vue';
import { drop, read, write } from '@/diff/drafts';

const props = defineProps<{
  start?: string;
  label: string;
  /// Where an unfinished remark is kept. Without it, nothing is kept.
  draft?: string;
}>();
const emit = defineEmits<{ save: [body: string]; cancel: [] }>();

const body = ref(props.start ?? (props.draft ? read(props.draft) : ''));
const area = ref<HTMLTextAreaElement | null>(null);

// A box that opens is a box the reader asked for, so the keyboard goes into
// it. A box that comes back with an unfinished remark does not take the
// keyboard: the reader opened a file, not that box.
onMounted(() => {
  if (body.value === '') {
    area.value?.focus();
  }
});

// Every key stroke, so that opening another file costs nothing. The store
// is the browser's own: an unfinished remark is not a comment.
watch(body, (text) => {
  if (props.draft) {
    write(props.draft, text);
  }
});

function save() {
  if (body.value.trim() === '') {
    return;
  }
  forget();
  emit('save', body.value);
}

function forget() {
  if (props.draft) {
    drop(props.draft);
  }
}
</script>

<template>
  <form class="talk-box" @submit.prevent="save">
    <p class="talk-head">{{ label }}</p>

    <div class="talk-body">
      <label class="sr-only" :for="`box-${label}`">{{ label }}</label>
      <!-- Ctrl+S is the browser's own save, so this one is prevented. -->
      <textarea
        :id="`box-${label}`"
        ref="area"
        v-model="body"
        class="talk-text"
        :placeholder="label"
        @keydown.ctrl.enter="save"
        @keydown.ctrl.s.prevent="save"
      ></textarea>
    </div>

    <div class="talk-foot">
      <span class="talk-hint">Markdown. Ctrl+Enter or Ctrl+S saves.</span>
      <span class="spacer"></span>
      <button
        type="button"
        class="action"
        @click="
          forget();
          emit('cancel');
        "
      >
        Cancel
      </button>
      <button type="submit" class="action" :disabled="body.trim() === ''">Save</button>
    </div>
  </form>
</template>
