<script setup lang="ts">
// The trace of the server, as `--trace` prints it, without a restart.
//
// A reader who waits wants to know on what. The server keeps its last lines,
// and this window shows them, and the new ones as they come.

import { nextTick, onBeforeUnmount, onMounted, ref } from 'vue';
import { api } from '@/api/client';
import type { TraceLine, Waiting } from '@/api/types';

const emit = defineEmits<{ close: [] }>();

/// The window keeps as many lines as the server does.
const KEPT = 2000;

const lines = ref<TraceLine[]>([]);
const waiting = ref<Waiting[]>([]);
const body = ref<HTMLElement | null>(null);
let next = 0;
let timer: ReturnType<typeof setInterval> | undefined;

async function read() {
  try {
    // The reader who scrolled up to read stays there.
    const element = body.value;
    const atEnd = !element || element.scrollTop + element.clientHeight >= element.scrollHeight - 4;
    const [found, now] = await Promise.all([api.trace(next), api.activity()]);
    next = found.next - 1;
    waiting.value = now.waiting;
    if (found.lines.length > 0) {
      lines.value = [...lines.value, ...found.lines].slice(-KEPT);
      if (atEnd) {
        await nextTick();
        body.value?.scrollTo({ top: body.value.scrollHeight });
      }
    }
  } catch {
    // The server stopped: the page says so elsewhere.
  }
}

function seconds(at: number): string {
  return `${at.toFixed(3)}s`;
}

function cost(ms: number | null): string {
  return ms === null ? '' : `${ms.toFixed(1)}ms`;
}

async function copy() {
  const text = lines.value
    .map((line) => `${seconds(line.at)} ${cost(line.ms)} ${line.what}`)
    .join('\n');
  await navigator.clipboard?.writeText(text).catch(() => undefined);
}

function onKey(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    emit('close');
  }
  event.stopImmediatePropagation();
}

onMounted(() => {
  window.addEventListener('keydown', onKey, true);
  void read();
  timer = setInterval(() => void read(), 1000);
});
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKey, true);
  clearInterval(timer);
});
</script>

<template>
  <div class="veil" @click.self="emit('close')">
    <div class="panel trace-panel" role="dialog" aria-modal="true" aria-label="Trace">
      <h2 class="panel-title">Trace</h2>
      <p class="fix-note">
        What the server did, and what each piece of work cost: the lines of
        <code>qreview --trace</code>.
      </p>
      <ul v-if="waiting.length > 0" class="trace-waiting" role="status">
        <li v-for="call in waiting" :key="call.what">
          Waiting {{ (call.ms / 1000).toFixed(1) }}s on {{ call.what }}
        </li>
      </ul>
      <div ref="body" class="trace-body">
        <p v-for="line in lines" :key="line.n" class="trace-line">
          <span class="trace-at">{{ seconds(line.at) }}</span>
          <span class="trace-ms" :class="line.ms !== null && line.ms >= 1000 ? 'is-slow' : ''">{{
            cost(line.ms)
          }}</span>
          <span class="trace-what">{{ line.what }}</span>
        </p>
      </div>
      <p class="fix-keys clean-keys">
        <button type="button" class="action" @click="copy">Copy</button>
        <button type="button" class="action" @click="emit('close')">Close</button>
        <span class="clean-hint"><kbd>Esc</kbd> closes</span>
      </p>
    </div>
  </div>
</template>
