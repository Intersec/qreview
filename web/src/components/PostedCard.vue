<script setup lang="ts">
// A remark already posted on Gerrit. Read only, and it says so: qreview
// never writes to the server, so there is nothing to edit and nothing to
// delete here.

import { render } from '@/diff/markdown';
import type { PostedComment } from '@/api/types';

defineProps<{ comment: PostedComment }>();
</script>

<template>
  <article class="talk-box posted-box">
    <p class="talk-head">
      <span class="talk-who">{{ comment.author }}</span>
      <span class="talk-tag">Gerrit</span>
      <span class="spacer"></span>
      <span>patch set {{ comment.patchSet }}</span>
    </p>

    <div class="talk-body">
      <!-- eslint-disable-next-line vue/no-v-html -- sanitized in diff/markdown.ts -->
      <div class="prose-comment" v-html="render(comment.body)"></div>
    </div>
  </article>
</template>
