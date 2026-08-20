<script setup lang="ts">
import { computed } from 'vue';
import { segments } from '@/diff/segments';
import type { Row } from '@/api/types';

const props = defineProps<{
  row: Row | null;
  /// Which line number this column carries.
  side: 'old' | 'new';
  /// True when clicking the number writes a comment there.
  commentable?: boolean;
}>();
const emit = defineEmits<{ comment: [] }>();

const kind = computed(() => props.row?.kind ?? 'empty');
const number = computed(() => {
  if (!props.row) {
    return '';
  }
  return (props.side === 'old' ? props.row.oldLine : props.row.newLine) ?? '';
});
</script>

<template>
  <td
    class="gutter"
    :class="[`gutter-${kind}`, commentable && number !== '' ? 'gutter-comment' : '']"
    :title="commentable && number !== '' ? 'Comment on this line' : undefined"
    @click="commentable && number !== '' ? emit('comment') : undefined"
  >
    {{ number }}
  </td>
  <td class="code-cell" :class="`row-${kind}`" :data-kind="kind">
    <template v-if="row">
      <span
        v-for="(seg, s) in segments(row)"
        :key="s"
        :class="[seg.cls, seg.changed ? 'word' : '']"
        >{{ seg.text }}</span
      >
      <span v-if="row.text === ''">&#8203;</span>
      <span v-if="row.noNewline" class="no-newline" title="no newline at the end of the file"
        >↵?</span
      >
    </template>
    <slot />
  </td>
</template>
