<script setup lang="ts">
import { computed } from 'vue';
import { segments, type Mark } from '@/diff/segments';
import type { Row } from '@/api/types';

const props = defineProps<{
  row: Row | null;
  /// Which line number this column carries.
  side: 'old' | 'new';
  /// True when clicking the number writes a comment there.
  commentable?: boolean;
  /// The part of the row a comment covers, if any.
  mark?: Mark;
}>();
const emit = defineEmits<{ comment: [] }>();

const kind = computed(() => props.row?.kind ?? 'empty');
const number = computed(() => {
  if (!props.row) {
    return '';
  }
  return (props.side === 'old' ? props.row.oldLine : props.row.newLine) ?? '';
});

// Which line the text of this cell belongs to. A removed line is read on the
// old side, everything else on the new one. The column is not the answer: in
// the side by side view the left column of a context line still shows the
// new side of it.
const owner = computed(() => {
  if (!props.row) {
    return null;
  }
  const side = props.row.kind === 'remove' ? 'old' : 'new';
  const line = props.row.kind === 'remove' ? props.row.oldLine : props.row.newLine;

  return line === null ? null : { side, line };
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
  <td
    class="code-cell"
    :class="`row-${kind}`"
    :data-kind="kind"
    :data-side="owner?.side"
    :data-line="owner?.line"
  >
    <template v-if="row">
      <span
        v-for="(seg, s) in segments(row, mark)"
        :key="s"
        :class="[seg.cls, seg.changed ? 'word' : '', seg.marked ? 'in-range' : '']"
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
