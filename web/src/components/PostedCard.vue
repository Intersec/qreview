<script setup lang="ts">
// A thread already posted on Gerrit: the remark, and the replies that
// Gerrit holds under it. It is edited in Gerrit, so this card has no edit
// and no delete. A resolved thread folds, as a done thread does.

import { computed, inject, ref } from 'vue';
import { render } from '@/diff/markdown';
import { THREADS, isResolved } from '@/diff/threads';
import type { PostedComment } from '@/api/types';

const props = defineProps<{
  comment: PostedComment;
  /// Set when the line it spoke of is not in this version.
  stranded?: boolean;
}>();

const threads = inject(THREADS, null);

const replies = computed(() => threads?.postedRepliesOf?.(props.comment.id) ?? []);
const resolved = computed(() => isResolved(props.comment, replies.value));
/// Null until the reader folds or unfolds it, so the state decides.
const foldedByHand = ref<boolean | null>(null);
const folded = computed(() => foldedByHand.value ?? resolved.value);

/// The place it was posted on, which this version no longer has.
const place = computed(() =>
  props.comment.line === null ? props.comment.file : `${props.comment.file}:${props.comment.line}`,
);

function firstLine(text: string): string {
  return text.split('\n', 1)[0] ?? '';
}
</script>

<template>
  <article
    class="talk-box posted-box"
    :class="[
      stranded ? 'talk-stranded' : '',
      resolved ? 'talk-done' : '',
      resolved && folded ? 'talk-folded' : '',
    ]"
  >
    <p class="talk-head">
      <button
        v-if="resolved"
        type="button"
        class="talk-fold"
        :aria-expanded="!folded"
        :title="folded ? 'Show this resolved thread' : 'Fold this resolved thread'"
        @click="foldedByHand = !folded"
      >
        <span>{{ folded ? '▸' : '▾' }}</span>
        <span class="talk-who">{{ comment.author }}</span>
        <span class="talk-tag">Gerrit</span>
        <span class="talk-tag">resolved</span>
        <span v-if="folded" class="talk-summary">{{ firstLine(comment.body) }}</span>
        <span v-if="folded && replies.length" class="talk-summary"
          >· {{ replies.length }} {{ replies.length === 1 ? 'reply' : 'replies' }}</span
        >
      </button>
      <template v-else>
        <span class="talk-who">{{ comment.author }}</span>
        <span class="talk-tag">Gerrit</span>
        <span v-if="comment.unresolved !== null" class="talk-tag">unresolved</span>
      </template>
      <span v-if="stranded" class="talk-tag">no line here</span>
      <code v-if="stranded" class="was-on">{{ place }}</code>
      <span class="spacer"></span>
      <span>patch set {{ comment.patchSet }}</span>
    </p>

    <template v-if="!folded">
      <div class="talk-body">
        <!-- eslint-disable-next-line vue/no-v-html -- sanitized in diff/markdown.ts -->
        <div class="prose-comment" v-html="render(comment.body)"></div>
      </div>
      <div v-for="reply in replies" :key="reply.id" class="talk-reply">
        <p class="talk-reply-head">
          <span class="talk-who">{{ reply.author }}</span>
        </p>
        <!-- eslint-disable-next-line vue/no-v-html -- sanitized in diff/markdown.ts -->
        <div class="prose-comment" v-html="render(reply.body)"></div>
      </div>
    </template>
  </article>
</template>
