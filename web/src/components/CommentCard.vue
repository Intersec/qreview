<script setup lang="ts">
import { computed, ref } from 'vue';
import CommentBox from './CommentBox.vue';
import { render } from '@/diff/markdown';
import { isCurrent } from '@/diff/versions';
import type { Comment, PatchSet } from '@/api/types';

const props = defineProps<{
  comment: Comment;
  /// True on a version that is not the newest. An older version is history:
  /// it is read, and written on from the newest one.
  readOnly?: boolean;
  /// Set when the line this remark spoke of is not in this version. The card
  /// then stands at the top of its file rather than on a line, and says so.
  stranded?: boolean;
  /// The sha the change carries now. A remark written on any other one is a
  /// previous remark: it is counted nowhere and exported nowhere.
  at: string;
  /// The versions of the change, so a previous remark can name the patch set
  /// it belongs to as well as the sha. A sha alone is a needle in a reflog.
  sets?: PatchSet[];
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

/// The version a previous remark was written on. A current remark says
/// nothing: it belongs to the code under it.
const version = computed(() =>
  isCurrent(props.comment, props.at) ? '' : props.comment.commit.slice(0, 8),
);

/// The patch set that version was, when the change still has it in its list.
const patchSet = computed(
  () => props.sets?.find((set) => set.commit === props.comment.commit)?.number,
);

/// A remark that cannot be edited or deleted.
///
/// A previous remark belongs to a round that is over, and the version it
/// speaks of is not the one on the screen. It is a record of that round, and
/// a record that can be rewritten is not one. Reading an older patch set
/// freezes the current remarks too: the newest version is where a review is
/// written.
const frozen = computed(() => props.readOnly || version.value !== '');
</script>

<template>
  <article
    class="talk-box"
    :class="[stranded ? 'talk-stranded' : '', version ? 'talk-previous' : '']"
  >
    <p class="talk-head">
      <span>{{ when(comment) }}</span>
      <span v-if="version" class="talk-tag">previous</span>
      <span v-if="stranded" class="talk-tag">no line here</span>
      <code v-if="stranded" class="was-on">{{ place }}</code>
      <span class="spacer"></span>
      <span v-if="version && patchSet">patch set {{ patchSet }} · {{ version }}</span>
      <span v-else-if="version">{{ version }}</span>
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

    <div v-if="!editing && !frozen" class="talk-foot">
      <span class="spacer"></span>
      <button type="button" class="action" @click="editing = true">Edit</button>
      <button type="button" class="action" @click="emit('remove', comment.id)">Delete</button>
    </div>
  </article>
</template>
