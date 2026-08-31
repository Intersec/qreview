<script setup lang="ts">
// What sits under one place of the code: the remarks written there, and the
// box when one is open. A row of the side by side view has two of these,
// one per column, so a remark stays on the side it speaks of.

import CommentBox from './CommentBox.vue';
import CommentCard from './CommentCard.vue';
import PostedCard from './PostedCard.vue';
import type { Comment, PatchSet, PostedComment } from '@/api/types';

defineProps<{
  comments: Comment[];
  /// What Gerrit already holds here. Read only, and first: it was written
  /// before the remarks of this session.
  posted: PostedComment[];
  /// True when a box is open on this place.
  writing: boolean;
  /// True on a version that is not the newest. History is read, not edited.
  readOnly: boolean;
  /// The sha the change carries now, so a previous remark says so.
  at: string;
  /// The versions of the change, so a previous remark names its patch set.
  sets: PatchSet[];
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
  <PostedCard v-for="remark in posted" :key="remark.id" :comment="remark" />
  <CommentCard
    v-for="comment in comments"
    :key="comment.id"
    :comment="comment"
    :at="at"
    :sets="sets"
    :read-only="readOnly"
    @edit="(id, body) => emit('edit', id, body)"
    @remove="(id) => emit('remove', id)"
  />
  <CommentBox
    v-if="writing && !readOnly"
    :draft="draft"
    :label="label"
    @save="(body) => emit('save', body)"
    @cancel="emit('cancel')"
  />
</template>
