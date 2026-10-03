<script setup lang="ts">
// Confirm Clean before it deletes.
//
// A reader clicks a button to learn what it does. The Undo alone left three
// seconds to read a notice and react, which a curious click does not.

import { onBeforeUnmount, onMounted } from 'vue';
import type { Comment } from '@/api/types';

defineProps<{
  /// The done threads that Clean deletes.
  done: Comment[];
}>();
const emit = defineEmits<{ clean: []; close: [] }>();

function place(comment: Comment): string {
  const anchor = comment.anchor;
  if (!anchor) {
    return 'the change';
  }
  return anchor.startLine === null ? anchor.file : `${anchor.file}:${anchor.startLine}`;
}

function firstLine(comment: Comment): string {
  return comment.body.split('\n', 1)[0] ?? '';
}

/// The dialog owns the keyboard while it stands, like the one of Fix.
function onKey(event: KeyboardEvent) {
  if (event.key === 'Enter') {
    event.preventDefault();
    emit('clean');
  } else if (event.key === 'Escape') {
    emit('close');
  }
  event.stopImmediatePropagation();
}

onMounted(() => window.addEventListener('keydown', onKey, true));
onBeforeUnmount(() => window.removeEventListener('keydown', onKey, true));
</script>

<template>
  <div class="veil" @click.self="emit('close')">
    <div class="panel fix-panel" role="dialog" aria-modal="true" aria-label="Clean this change">
      <h2 class="panel-title">Delete the done threads</h2>
      <p class="fix-note">
        Clean deletes {{ done.length }} done {{ done.length === 1 ? 'thread' : 'threads' }} of this
        change, with their replies, for good:
      </p>
      <ul class="fix-list">
        <li v-for="comment in done" :key="comment.id">
          <code>{{ place(comment) }}</code> {{ firstLine(comment) }}
        </li>
      </ul>
      <p class="fix-keys">
        <button type="button" class="action" @click="emit('clean')">Delete</button>
        <button type="button" class="action" @click="emit('close')">Cancel</button>
        <kbd>Enter</kbd> deletes · <kbd>Esc</kbd> closes
      </p>
    </div>
  </div>
</template>
