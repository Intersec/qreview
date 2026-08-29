<script setup lang="ts">
// A remark already posted on Gerrit. Read only, and it says so: qreview
// never writes to the server, so there is nothing to edit and nothing to
// delete here.

import { computed } from 'vue';
import { render } from '@/diff/markdown';
import type { PostedComment } from '@/api/types';

const props = defineProps<{
  comment: PostedComment;
  /// Set when the line it spoke of is not in this version.
  stranded?: boolean;
}>();

/// The place it was posted on, which this version no longer has.
const place = computed(() =>
  props.comment.line === null ? props.comment.file : `${props.comment.file}:${props.comment.line}`,
);
</script>

<template>
  <article class="talk-box posted-box" :class="stranded ? 'talk-stranded' : ''">
    <p class="talk-head">
      <span class="talk-who">{{ comment.author }}</span>
      <span class="talk-tag">Gerrit</span>
      <span v-if="stranded" class="talk-tag">no line here</span>
      <code v-if="stranded" class="was-on">{{ place }}</code>
      <span class="spacer"></span>
      <span>patch set {{ comment.patchSet }}</span>
    </p>

    <div class="talk-body">
      <!-- eslint-disable-next-line vue/no-v-html -- sanitized in diff/markdown.ts -->
      <div class="prose-comment" v-html="render(comment.body)"></div>
    </div>
  </article>
</template>
