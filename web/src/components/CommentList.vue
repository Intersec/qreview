<script setup lang="ts">
// What the session holds, as a list to walk.
//
// The change on the screen comes first, because that is what the reader is
// working on. The rest follows in the order of the export, so the pane and
// the text handed to a session say the same thing in the same order.

import { computed, ref } from 'vue';
import { label } from '@/diff/paths';
import type { ChangeComments, Comment, Side } from '@/api/types';

const props = defineProps<{
  written: ChangeComments[];
  openKey: string | null;
  /// How tall the reader made it, in pixels.
  height: number;
}>();
const emit = defineEmits<{
  go: [key: string, file: string, side: Side, line: number | null];
}>();

const folded = ref(false);

const total = computed(() =>
  props.written.reduce((sum, change) => sum + change.comments.length, 0),
);

/// The change being read first, the others in the order they came.
const groups = computed(() => {
  const here = props.written.filter((change) => change.key === props.openKey);
  const rest = props.written.filter((change) => change.key !== props.openKey);

  return [...here, ...rest];
});

/// Where a comment sits, short enough for a narrow pane.
function place(comment: Comment): string {
  const anchor = comment.anchor;
  if (!anchor) {
    return 'the change';
  }
  const cut = anchor.file.lastIndexOf('/');
  const name = anchor.file.startsWith('/') ? label(anchor.file) : anchor.file.slice(cut + 1);
  if (anchor.startLine === null) {
    return name;
  }
  const end = anchor.endLine ?? anchor.startLine;

  return end > anchor.startLine
    ? `${name}:${anchor.startLine}-${end}`
    : `${name}:${anchor.startLine}`;
}

/// The first line of the body, which is what a list has room for.
function gist(comment: Comment): string {
  return comment.body.split('\n').find((line) => line.trim() !== '') ?? '';
}

function go(change: ChangeComments, comment: Comment) {
  const anchor = comment.anchor;
  emit('go', change.key, anchor?.file ?? '', anchor?.side ?? 'new', anchor?.startLine ?? null);
}
</script>

<template>
  <section
    v-if="total > 0"
    class="comment-list"
    :class="folded ? 'is-folded' : ''"
    :style="folded ? undefined : { height: `${height}px` }"
  >
    <button
      type="button"
      class="pane-title list-head"
      :aria-expanded="!folded"
      @click="folded = !folded"
    >
      <span>Comments · {{ total }}</span>
      <span aria-hidden="true">{{ folded ? '▸' : '▾' }}</span>
    </button>

    <div v-if="!folded" class="list-body">
      <template v-for="change in groups" :key="change.key">
        <p class="list-change" :title="change.subject">
          <span v-if="change.key === openKey" class="tag">here</span>
          {{ change.subject }}
        </p>
        <button
          v-for="comment in change.comments"
          :key="comment.id"
          type="button"
          class="row-button list-row"
          :title="`${comment.anchor?.file ?? 'the change'} — ${gist(comment)}`"
          @click="go(change, comment)"
        >
          <code class="list-place">{{ place(comment) }}</code>
          <span class="list-gist">{{ gist(comment) }}</span>
        </button>
      </template>
    </div>
  </section>
</template>
