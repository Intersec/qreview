<script setup lang="ts">
// Ask the agent to fix the change on the screen.
//
// A dialog rather than a key that sends at once: a key typed into a box that
// has not taken the focus yet would otherwise start the work. Here it only
// opens this, and nothing leaves without Enter.

import { onBeforeUnmount, onMounted } from 'vue';
import type { Comment } from '@/api/types';

const props = defineProps<{
  /// The open threads the agent will deal with.
  open: Comment[];
  /// The threads that wait for the reader. Any of them stops the request.
  waiting: Comment[];
}>();
const emit = defineEmits<{ send: []; jump: [comment: Comment]; close: [] }>();

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

function confirm() {
  const first = props.waiting[0];
  if (first) {
    emit('jump', first);
  } else if (props.open.length > 0) {
    emit('send');
  }
}

/// The dialog owns the keyboard while it stands: no shortcut of the review
/// runs behind it.
function onKey(event: KeyboardEvent) {
  if (event.key === 'Enter') {
    event.preventDefault();
    confirm();
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
    <div class="panel fix-panel" role="dialog" aria-modal="true" aria-label="Fix this change">
      <template v-if="waiting.length > 0">
        <h2 class="panel-title">The agent waits for you</h2>
        <p class="fix-note">
          Answer {{ waiting.length === 1 ? 'this question' : 'these questions' }} first. Nothing is
          sent.
        </p>
        <ul class="fix-list">
          <li v-for="comment in waiting" :key="comment.id">
            <code>{{ place(comment) }}</code> {{ firstLine(comment) }}
          </li>
        </ul>
        <p class="fix-keys"><kbd>Enter</kbd> goes to the first one · <kbd>Esc</kbd> closes</p>
      </template>
      <template v-else-if="open.length > 0">
        <h2 class="panel-title">Fix this change</h2>
        <p class="fix-note">
          The agent deals with {{ open.length }} open
          {{ open.length === 1 ? 'thread' : 'threads' }}:
        </p>
        <ul class="fix-list">
          <li v-for="comment in open" :key="comment.id">
            <code>{{ place(comment) }}</code> {{ firstLine(comment) }}
          </li>
        </ul>
        <p class="fix-keys"><kbd>Enter</kbd> sends · <kbd>Esc</kbd> closes</p>
      </template>
      <template v-else>
        <h2 class="panel-title">Nothing to fix</h2>
        <p class="fix-note">This change has no open thread.</p>
        <p class="fix-keys"><kbd>Esc</kbd> closes</p>
      </template>
    </div>
  </div>
</template>
