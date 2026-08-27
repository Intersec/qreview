<script setup lang="ts">
import { computed, inject, ref } from 'vue';
import { HOVERED, occurrences, type Run } from '@/diff/hover';
import { segments, type Mark } from '@/diff/segments';
import type { Row } from '@/api/types';

/// The same empty array every time, so a row that does not carry the word
/// keeps the pieces it already has and is not painted again.
const NONE: Run[] = [];

/// A row outside a diff pane is on no word.
const NOTHING = ref<string | null>(null);

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

const hovered = inject(HOVERED, NOTHING);

/// Where the word the pointer is on stands in this row.
const same = computed<Run[]>(() => {
  const word = hovered.value;
  if (word === null || !props.row) {
    return NONE;
  }
  const found = occurrences(props.row.text, word);

  return found.length > 0 ? found : NONE;
});

/// The pieces to paint. A computed rather than a call in the template: it
/// is what keeps a row still when the pointer moves over another one.
const parts = computed(() => (props.row ? segments(props.row, props.mark, same.value) : []));

const kind = computed(() => props.row?.kind ?? 'empty');

// The old column speaks of the version before the change. A context line
// stands in both, so the hint says which one the click writes about.
const hint = computed(() =>
  props.side === 'old' ? 'Comment on this line, before the change' : 'Comment on this line',
);
const number = computed(() => {
  if (!props.row) {
    return '';
  }
  return (props.side === 'old' ? props.row.oldLine : props.row.newLine) ?? '';
});

// Which line the text of this cell belongs to.
//
// A column speaks for its own version of the file, so the left column of a
// context line is the old side of it. The unified view has one column and
// draws a removed line in it, so a cell with no new number falls back to the
// old side rather than belonging to nothing.
const owner = computed(() => {
  if (!props.row) {
    return null;
  }
  const line = props.side === 'old' ? props.row.oldLine : props.row.newLine;
  if (line !== null) {
    return { side: props.side, line };
  }
  if (props.side === 'new' && props.row.oldLine !== null) {
    return { side: 'old' as const, line: props.row.oldLine };
  }
  return null;
});
</script>

<template>
  <td
    class="gutter"
    :class="[`gutter-${kind}`, commentable && number !== '' ? 'gutter-comment' : '']"
    :title="commentable && number !== '' ? hint : undefined"
    :data-column="side"
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
    :data-column="side"
  >
    <template v-if="row">
      <span
        v-for="(seg, s) in parts"
        :key="s"
        :class="[
          seg.cls,
          seg.changed ? 'word' : '',
          seg.marked ? 'in-range' : '',
          seg.same ? 'same-word' : '',
        ]"
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
