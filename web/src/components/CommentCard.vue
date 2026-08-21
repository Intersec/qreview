<script setup lang="ts">
import { ref } from 'vue';
import CommentBox from './CommentBox.vue';
import { render } from '@/diff/markdown';
import type { Comment } from '@/api/types';

defineProps<{ comment: Comment }>();
const emit = defineEmits<{ edit: [id: string, body: string]; remove: [id: string] }>();

const editing = ref(false);

function when(comment: Comment): string {
  return comment.createdAt.slice(0, 16).replace('T', ' ');
}
</script>

<template>
  <article class="talk-box">
    <p class="talk-head">
      <span>{{ when(comment) }}</span>
      <span class="spacer"></span>
      <span>patch set {{ comment.patchSet }}</span>
    </p>

    <div class="talk-body">
      <CommentBox
        v-if="editing"
        :start="comment.body"
        label="Edit the comment"
        @save="
          (body) => {
            emit('edit', comment.id, body);
            editing = false;
          }
        "
        @cancel="editing = false"
      />
      <!-- eslint-disable-next-line vue/no-v-html -- sanitized in diff/markdown.ts -->
      <div v-else class="prose-comment" v-html="render(comment.body)"></div>
    </div>

    <div v-if="!editing" class="talk-foot">
      <span class="spacer"></span>
      <button type="button" class="action" @click="editing = true">Edit</button>
      <button type="button" class="action" @click="emit('remove', comment.id)">Delete</button>
    </div>
  </article>
</template>
