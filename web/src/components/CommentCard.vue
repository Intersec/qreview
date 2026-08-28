<script setup lang="ts">
import { computed, ref } from 'vue';
import CommentBox from './CommentBox.vue';
import { render } from '@/diff/markdown';
import type { Comment } from '@/api/types';

const props = defineProps<{
  comment: Comment;
  /// True on a version that is not the newest. An older version is history:
  /// it is read, and written on from the newest one.
  readOnly?: boolean;
  /// Set when the line this remark spoke of is not in this version. The card
  /// then stands at the top of its file rather than on a line, and says so.
  stranded?: 'answered' | 'lost';
  /// The commit being read, so a remark written on another one says which.
  at: string;
}>();
const emit = defineEmits<{ edit: [id: string, body: string]; remove: [id: string] }>();

const editing = ref(false);

function when(comment: Comment): string {
  return comment.createdAt.slice(0, 16).replace('T', ' ');
}

/// The place the remark was written on, which this version no longer has.
const place = computed(() => {
  const anchor = props.comment.anchor;
  if (!anchor) {
    return '';
  }
  return anchor.startLine === null ? anchor.file : `${anchor.file}:${anchor.startLine}`;
});

/// The version it was written on, when that is not the one being read. A
/// remark of this version says nothing: it is the one the code belongs to.
const version = computed(() => {
  const { commit } = props.comment;

  return commit === '' || commit === props.at ? '' : commit.slice(0, 8);
});
</script>

<template>
  <article
    class="talk-box"
    :class="[stranded ? 'talk-stranded' : '', version ? 'talk-earlier' : '']"
  >
    <p class="talk-head">
      <span>{{ when(comment) }}</span>
      <span v-if="stranded === 'answered'" class="talk-tag">answered</span>
      <span v-else-if="stranded" class="talk-tag">not placed</span>
      <code v-if="stranded" class="was-on">{{ place }}</code>
      <span class="spacer"></span>
      <span v-if="version">on {{ version }}</span>
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

    <div v-if="!editing && !readOnly" class="talk-foot">
      <span class="spacer"></span>
      <button type="button" class="action" @click="editing = true">Edit</button>
      <button type="button" class="action" @click="emit('remove', comment.id)">Delete</button>
    </div>
  </article>
</template>
