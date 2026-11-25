<script setup lang="ts">
// The message of the version being read, in the middle of the change page
// as Gerrit shows it: a box as wide as the message, not as the page.

import { ref, watch } from 'vue';
import { api } from '@/api/client';

const props = defineProps<{
  changeKey: string | null;
  /// The patch set being read. Absent is the newest.
  patchSet: number | undefined;
}>();

/// A message longer than this is cut: the box is a summary, the file is
/// the whole of it.
const LINES = 200;

const text = ref('');

watch(
  () => [props.changeKey, props.patchSet] as const,
  async ([key, ps]) => {
    if (!key) {
      text.value = '';
      return;
    }
    try {
      const rows = await api.lines(key, '/COMMIT_MSG', 1, LINES, ps);
      // A newer choice of the reader wins over this answer.
      if (key === props.changeKey && ps === props.patchSet) {
        text.value = rows.map((row) => row.text).join('\n');
      }
    } catch {
      text.value = '';
    }
  },
  { immediate: true },
);
</script>

<template>
  <section class="commit-message">
    <pre>{{ text }}</pre>
  </section>
</template>
