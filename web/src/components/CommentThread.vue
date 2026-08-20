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
  <article class="talk-box" :class="first.resolved ? 'opacity-60' : ''">
    <div v-for="comment in all" :key="comment.id">
      <p class="talk-head">
        <span class="talk-who">{{ comment.author }}</span>
        <span>{{ when(comment) }}</span>
        <span v-if="comment.draft" class="talk-tag">draft</span>
        <span v-if="first.resolved && comment.id === first.id" class="talk-tag">resolved</span>
        <span class="spacer"></span>
        <span>patch set {{ comment.patchSet }}</span>
      </p>

      <div class="talk-body">
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
        <div v-else class="prose-comment" v-html="render(comment.body)"></div>
      </div>

      <div v-if="editing !== comment.id" class="talk-foot">
        <button type="button" class="action" @click="editing = comment.id">Edit</button>
        <button type="button" class="action" @click="emit('remove', comment.id)">Delete</button>
        <span class="spacer"></span>
        <template v-if="comment.id === first.id">
          <button type="button" class="action" @click="replying = !replying">Reply</button>
          <button type="button" class="action" @click="emit('resolve', first.id, !first.resolved)">
            {{ first.resolved ? 'Reopen' : 'Resolve' }}
          </button>
        </template>
      </div>
    </div>

    <div v-if="replying" class="talk-body">
      <CommentBox
        label="Answer"
        @save="
          (body, draft) => {
            emit('reply', first.id, body, draft);
            replying = false;
          }
        "
        @cancel="replying = false"
      />
    </div>
  </article>
</template>
