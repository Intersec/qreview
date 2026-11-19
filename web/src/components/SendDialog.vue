<script setup lang="ts">
// Send the remarks staged for Gerrit, of the change on the screen or of the
// whole series. Each one becomes a draft, which the reader publishes in
// Gerrit. See section 5.8 of the design.
//
// A dialog, as for Fix: the list says exactly what leaves this tool, and
// nothing leaves without Enter.

import { computed, onBeforeUnmount, onMounted } from 'vue';
import type { Comment, GerritStatus } from '@/api/types';

type Remark = { key: string; comment: Comment };

const props = defineProps<{
  /// The whole series, rather than the change on the screen.
  series: boolean;
  /// The remarks staged for Gerrit.
  staged: Remark[];
  /// What the REST API of Gerrit allows this run.
  status: GerritStatus | null;
  /// The remarks that Gerrit refused at the last send, by id.
  failed: Map<string, string>;
  /// True while the send runs.
  sending: boolean;
  /// The subject of a change, which heads its remarks in the series.
  subjectOf: (key: string) => string | undefined;
}>();
const emit = defineEmits<{ send: []; close: [] }>();

const what = computed(() => (props.series ? 'the series' : 'this change'));
const allowed = computed(() => props.status?.rest === 'ok');

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

/// The remarks change by change, in the order of the series.
const groups = computed(() => {
  const out: { key: string; subject: string; remarks: Remark[] }[] = [];
  for (const remark of props.staged) {
    const last = out[out.length - 1];
    if (last?.key === remark.key) {
      last.remarks.push(remark);
    } else {
      out.push({
        key: remark.key,
        subject: props.subjectOf(remark.key) ?? remark.key,
        remarks: [remark],
      });
    }
  }
  return out;
});

function confirm() {
  if (allowed.value && props.staged.length > 0 && !props.sending) {
    emit('send');
  }
}

/// The dialog owns the keyboard while it stands.
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
    <div
      class="panel fix-panel"
      role="dialog"
      aria-modal="true"
      :aria-label="`Send ${what} to Gerrit`"
    >
      <template v-if="staged.length === 0">
        <h2 class="panel-title">Nothing to send</h2>
        <p class="fix-note">
          {{ series ? 'The series has' : 'This change has' }} no remark for Gerrit.
        </p>
        <p class="fix-keys"><kbd>Esc</kbd> closes</p>
      </template>
      <template v-else>
        <h2 class="panel-title">Send {{ what }} to Gerrit</h2>
        <p class="fix-note">
          {{ staged.length }}
          {{ staged.length === 1 ? 'remark becomes a draft' : 'remarks become drafts' }}
          on Gerrit. Only you see a draft until you publish it there.
        </p>
        <ul class="fix-list">
          <template v-for="group in groups" :key="group.key">
            <li v-if="series" class="fix-change">{{ group.subject }}</li>
            <li v-for="remark in group.remarks" :key="remark.comment.id">
              <code>{{ place(remark.comment) }}</code>
              <span class="send-set">patch set {{ remark.comment.gerrit?.patchSet }}</span>
              {{ firstLine(remark.comment) }}
              <p v-if="failed.has(remark.comment.id)" class="send-failed" role="alert">
                {{ failed.get(remark.comment.id) }}
              </p>
            </li>
          </template>
        </ul>
        <p v-if="!allowed" class="send-failed" role="alert">
          {{ status?.reason ?? 'Gerrit takes no draft from this run.' }}
        </p>
        <p v-if="sending" class="fix-keys">Sending…</p>
        <p v-else-if="allowed" class="fix-keys"><kbd>Enter</kbd> sends · <kbd>Esc</kbd> closes</p>
        <p v-else class="fix-keys"><kbd>Esc</kbd> closes</p>
      </template>
    </div>
  </div>
</template>
