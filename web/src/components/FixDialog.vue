<script setup lang="ts">
// Ask the agent to fix the change on the screen, or the whole series.
//
// A dialog rather than a key that sends at once: a key typed into a box that
// has not taken the focus yet would otherwise start the work. Here it only
// opens this, and nothing leaves without Enter.

import { computed, onBeforeUnmount, onMounted } from 'vue';
import type { Comment } from '@/api/types';

type Thread = { key: string; comment: Comment };

const props = defineProps<{
  /// The whole series, rather than the change on the screen.
  series: boolean;
  /// The open threads the agent will deal with.
  open: Thread[];
  /// The threads that wait for the reader. Any of them stops the request.
  waiting: Thread[];
  /// The subject of a change, which heads its threads in the series.
  subjectOf: (key: string) => string | undefined;
}>();
const emit = defineEmits<{ send: []; jump: [thread: Thread]; close: [] }>();

const what = computed(() => (props.series ? 'the series' : 'this change'));

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

/// The threads change by change, in the order of the series. The list
/// comes in that order, so a change is one run of it.
function grouped(threads: Thread[]): { key: string; subject: string; threads: Thread[] }[] {
  const out: { key: string; subject: string; threads: Thread[] }[] = [];
  for (const thread of threads) {
    const last = out[out.length - 1];
    if (last?.key === thread.key) {
      last.threads.push(thread);
    } else {
      out.push({
        key: thread.key,
        subject: props.subjectOf(thread.key) ?? thread.key,
        threads: [thread],
      });
    }
  }
  return out;
}

const changes = computed(() => new Set(props.open.map((t) => t.key)).size);

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
    <div class="panel fix-panel" role="dialog" aria-modal="true" :aria-label="`Fix ${what}`">
      <template v-if="waiting.length > 0">
        <h2 class="panel-title">The agent waits for you</h2>
        <p class="fix-note">
          Answer {{ waiting.length === 1 ? 'this question' : 'these questions' }} first. Nothing is
          sent.
        </p>
        <ul class="fix-list">
          <template v-for="group in grouped(waiting)" :key="group.key">
            <li v-if="series" class="fix-change">{{ group.subject }}</li>
            <li v-for="thread in group.threads" :key="thread.comment.id">
              <code>{{ place(thread.comment) }}</code> {{ firstLine(thread.comment) }}
            </li>
          </template>
        </ul>
        <p class="fix-keys"><kbd>Enter</kbd> goes to the first one · <kbd>Esc</kbd> closes</p>
      </template>
      <template v-else-if="open.length > 0">
        <h2 class="panel-title">Fix {{ what }}</h2>
        <p class="fix-note">
          The agent deals with {{ open.length }} open {{ open.length === 1 ? 'thread' : 'threads'
          }}<template v-if="series">
            in {{ changes }} {{ changes === 1 ? 'change' : 'changes' }}</template
          >:
        </p>
        <ul class="fix-list">
          <template v-for="group in grouped(open)" :key="group.key">
            <li v-if="series" class="fix-change">{{ group.subject }}</li>
            <li v-for="thread in group.threads" :key="thread.comment.id">
              <code>{{ place(thread.comment) }}</code> {{ firstLine(thread.comment) }}
            </li>
          </template>
        </ul>
        <p class="fix-keys"><kbd>Enter</kbd> sends · <kbd>Esc</kbd> closes</p>
      </template>
      <template v-else>
        <h2 class="panel-title">Nothing to fix</h2>
        <p class="fix-note">{{ series ? 'The series has' : 'This change has' }} no open thread.</p>
        <p class="fix-keys"><kbd>Esc</kbd> closes</p>
      </template>
    </div>
  </div>
</template>
