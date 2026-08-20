<script setup lang="ts">
import { computed, ref } from 'vue';
import CommentBox from './CommentBox.vue';
import { render } from '@/diff/markdown';
import type { Comment } from '@/api/types';

const props = defineProps<{ first: Comment; replies: Comment[] }>();
const emit = defineEmits<{
  reply: [parentId: string, body: string, draft: boolean];
  edit: [id: string, body: string];
  remove: [id: string];
  resolve: [id: string, resolved: boolean];
}>();

const replying = ref(false);
const editing = ref<string | null>(null);

const all = computed(() => [props.first, ...props.replies]);

function when(comment: Comment): string {
  return comment.createdAt.slice(0, 16).replace('T', ' ');
}
</script>

<template>
  <article
    class="my-1 rounded border border-slate-300 bg-white p-2 text-sm dark:border-slate-600 dark:bg-slate-800"
    :class="first.resolved ? 'opacity-60' : ''"
  >
    <div
      v-for="comment in all"
      :key="comment.id"
      class="border-slate-200 py-1 dark:border-slate-700 [&+&]:border-t"
    >
      <p class="flex items-baseline gap-2 text-xs text-slate-500 dark:text-slate-400">
        <span class="font-medium text-slate-700 dark:text-slate-200">{{ comment.author }}</span>
        <span>{{ when(comment) }}</span>
        <span v-if="comment.draft" class="rounded bg-amber-200 px-1 text-amber-900">draft</span>
        <span v-if="comment.patchSet" class="ml-auto">patch set {{ comment.patchSet }}</span>
      </p>

      <CommentBox
        v-if="editing === comment.id"
        :start="comment.body"
        label="Edit the comment"
        @save="
          (body) => {
            emit('edit', comment.id, body);
            editing = null;
          }
        "
        @cancel="editing = null"
      />
      <!-- eslint-disable-next-line vue/no-v-html -- sanitized in diff/markdown.ts -->
      <div v-else class="prose-comment mt-1" v-html="render(comment.body)"></div>

      <p class="mt-1 flex gap-2 text-xs">
        <button type="button" class="underline" @click="editing = comment.id">Edit</button>
        <button type="button" class="underline" @click="emit('remove', comment.id)">Delete</button>
      </p>
    </div>

    <div class="mt-2 flex items-center gap-2 text-xs">
      <button
        type="button"
        class="rounded border border-slate-300 px-2 py-0.5 dark:border-slate-600"
        @click="replying = !replying"
      >
        Reply
      </button>
      <button
        type="button"
        class="rounded border border-slate-300 px-2 py-0.5 dark:border-slate-600"
        @click="emit('resolve', first.id, !first.resolved)"
      >
        {{ first.resolved ? 'Reopen' : 'Resolve' }}
      </button>
    </div>

    <CommentBox
      v-if="replying"
      class="mt-2"
      label="Answer"
      @save="
        (body, draft) => {
          emit('reply', first.id, body, draft);
          replying = false;
        }
      "
      @cancel="replying = false"
    />
  </article>
</template>
