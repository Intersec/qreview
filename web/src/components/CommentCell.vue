<script setup lang="ts">
// What sits under one place of the code: the remarks written there, and the
// box when one is open. A row of the side by side view has two of these,
// one per column, so a remark stays on the side it speaks of.

import CommentBox from './CommentBox.vue';
import CommentCard from './CommentCard.vue';
import type { Comment } from '@/api/types';

defineProps<{
  comments: Comment[];
  /// True when a box is open on this place.
  writing: boolean;
  /// Where the browser keeps what is typed and not saved.
  draft: string;
  label: string;
}>();
const emit = defineEmits<{
  save: [body: string];
  cancel: [];
  edit: [id: string, body: string];
  remove: [id: string];
}>();
</script>

<template>
  <CommentCard
    v-for="comment in comments"
    :key="comment.id"
    :comment="comment"
    @edit="(id, body) => emit('edit', id, body)"
    @remove="(id) => emit('remove', id)"
  />
  <CommentBox
    v-if="writing"
    :draft="draft"
    :label="label"
    @save="(body) => emit('save', body)"
    @cancel="emit('cancel')"
  />
</template>
