<script setup lang="ts">
import { computed, ref } from 'vue';
import CommentBox from './CommentBox.vue';
import CommentThread from './CommentThread.vue';
import type { Comment, NewComment } from '@/api/types';

const props = defineProps<{
  threads: { first: Comment; replies: Comment[] }[];
  file: string | null;
  lost: Comment[];
}>();
const emit = defineEmits<{
  add: [comment: NewComment];
  edit: [id: string, body: string];
  remove: [id: string];
  resolve: [id: string, resolved: boolean];
}>();

const writing = ref<'change' | 'file' | null>(null);

/// A comment on the change, and one on the file being read. A comment on a
/// line lives beside that line, in the diff.
const loose = computed(() =>
  props.threads.filter(
    (t) =>
      t.first.scope === 'change' ||
      (t.first.scope === 'file' && t.first.anchor?.file === props.file),
  ),
);

const unresolved = computed(() => props.threads.filter((t) => !t.first.resolved).length);
</script>

<template>
  <aside class="flex h-full flex-col gap-2 overflow-y-auto p-3">
    <h2 class="text-xs font-semibold uppercase tracking-wide text-slate-500 dark:text-slate-400">
      Comments · {{ threads.length }}
      <span v-if="unresolved">({{ unresolved }} open)</span>
    </h2>

    <div class="flex gap-2 text-xs">
      <button
        type="button"
        class="rounded border border-slate-300 px-2 py-1 dark:border-slate-600"
        @click="writing = writing === 'change' ? null : 'change'"
      >
        On the change
      </button>
      <button
        type="button"
        class="rounded border border-slate-300 px-2 py-1 disabled:opacity-50 dark:border-slate-600"
        :disabled="!file"
        @click="writing = writing === 'file' ? null : 'file'"
      >
        On this file
      </button>
    </div>

    <CommentBox
      v-if="writing"
      :label="writing === 'change' ? 'A remark about the change' : 'A remark about the file'"
      @save="
        (body, draft) => {
          emit('add', {
            scope: writing === 'change' ? 'change' : 'file',
            file: writing === 'file' && file ? file : undefined,
            side: writing === 'file' ? 'new' : undefined,
            body,
            draft,
          });
          writing = null;
        }
      "
      @cancel="writing = null"
    />

    <p v-if="loose.length === 0" class="text-sm text-slate-500 dark:text-slate-400">
      Nothing yet. A comment on a line is written from the line itself.
    </p>

    <section
      v-if="lost.length"
      class="rounded border border-amber-400 bg-amber-50 p-2 dark:border-amber-700 dark:bg-amber-950/40"
    >
      <h3 class="text-xs font-semibold uppercase tracking-wide text-amber-900 dark:text-amber-200">
        Could not be placed · {{ lost.length }}
      </h3>
      <p class="mt-1 text-xs text-slate-600 dark:text-slate-300">
        The line these were written on is not in this patch set. They are kept here rather than
        moved to a line nobody chose.
      </p>
      <ul class="mt-2 space-y-1">
        <li v-for="comment in lost" :key="comment.id" class="text-xs">
          <code>{{ comment.anchor?.file }}:{{ comment.anchor?.startLine }}</code>
          <span class="ml-1 text-slate-500 dark:text-slate-400">
            patch set {{ comment.patchSet }}
          </span>
          <p class="mt-0.5">{{ comment.body }}</p>
        </li>
      </ul>
    </section>

    <CommentThread
      v-for="thread in loose"
      :key="thread.first.id"
      :first="thread.first"
      :replies="thread.replies"
      @reply="(id, body, draft) => emit('add', { parentId: id, scope: 'change', body, draft })"
      @edit="(id, body) => emit('edit', id, body)"
      @remove="(id) => emit('remove', id)"
      @resolve="(id, value) => emit('resolve', id, value)"
    />
  </aside>
</template>
