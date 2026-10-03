<script setup lang="ts">
import { computed, inject, ref, watch } from 'vue';
import CommentBox from './CommentBox.vue';
import { render } from '@/diff/markdown';
import { THREADS, isBlocked } from '@/diff/threads';
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
const replying = ref(false);

/// Absent where no review provides it, as in a test of this card alone.
const threads = inject(THREADS, null);
const replies = computed(() => threads?.repliesOf(props.comment.id) ?? []);
/// The agent asked the reader, and waits for the answer.
const blocked = computed(() => isBlocked(replies.value));
const agent = computed(() => props.comment.author === 'agent');

/// A done thread is folded, and one click on its head unfolds it without
/// a change of state. A thread that waits for the reader is never folded.
const folded = ref(props.comment.done);
watch(
  () => props.comment.done,
  (done) => {
    folded.value = done;
  },
);
const shut = computed(() => folded.value && props.comment.done && !blocked.value);

/// The first line of the remark, for the head of a folded thread.
const summary = computed(() => props.comment.body.split('\n', 1)[0] ?? '');

function author(comment: Comment): string {
  return comment.author === 'agent' ? 'Agent' : 'You';
}

async function sendReply(body: string) {
  replying.value = false;
  await threads?.reply(props.comment.id, body);
}

async function toggleDone(event: Event) {
  await threads?.setDone(props.comment.id, (event.target as HTMLInputElement).checked);
}

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
    :class="[
      stranded ? 'talk-stranded' : '',
      version ? 'talk-previous' : '',
      agent ? 'talk-agent' : '',
      comment.done ? 'talk-done' : '',
      shut ? 'talk-folded' : '',
      blocked ? 'talk-blocked' : '',
    ]"
  >
    <p class="talk-head">
      <button
        v-if="comment.done && !blocked"
        type="button"
        class="talk-fold"
        :aria-expanded="!shut"
        :title="shut ? 'Show this done thread' : 'Fold this done thread'"
        @click="folded = !folded"
      >
        <span>{{ shut ? '▸' : '▾' }}</span>
        <span>{{ when(comment) }}</span>
        <span v-if="agent" class="talk-tag">agent</span>
        <span class="talk-tag">done</span>
        <span v-if="shut" class="talk-summary">{{ summary }}</span>
        <span v-if="shut && replies.length" class="talk-summary"
          >· {{ replies.length }} {{ replies.length === 1 ? 'reply' : 'replies' }}</span
        >
      </button>
      <template v-else>
        <span>{{ when(comment) }}</span>
        <span v-if="agent" class="talk-tag">agent</span>
        <span v-if="blocked" class="talk-tag talk-tag-waiting">waits for you</span>
      </template>
      <span v-if="version" class="talk-tag">previous</span>
      <span v-if="stranded" class="talk-tag">no line here</span>
      <code v-if="stranded" class="was-on">{{ place }}</code>
      <span class="spacer"></span>
      <span v-if="version && patchSet">patch set {{ patchSet }} · {{ version }}</span>
      <span v-else-if="version">{{ version }}</span>
    </p>

    <template v-if="!shut">
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

      <div
        v-for="answer in replies"
        :key="answer.id"
        class="talk-reply"
        :class="[
          answer.author === 'agent' ? 'talk-reply-agent' : '',
          answer.blocked && answer.author === 'agent' ? 'talk-reply-waiting' : '',
        ]"
      >
        <p class="talk-reply-head">
          <span class="talk-who">{{ author(answer) }}</span>
          <span>{{ when(answer) }}</span>
          <span v-if="answer.blocked && answer.author === 'agent'" class="talk-tag talk-tag-waiting"
            >question</span
          >
        </p>
        <!-- eslint-disable-next-line vue/no-v-html -- sanitized in diff/markdown.ts -->
        <div class="prose-comment" v-html="render(answer.body)"></div>
      </div>

      <div v-if="replying" class="talk-body">
        <CommentBox label="Reply" @save="sendReply" @cancel="replying = false" />
      </div>

      <div v-if="!editing && !replying && !frozen" class="talk-foot">
        <label v-if="threads" class="talk-done-box">
          <input type="checkbox" :checked="comment.done" @change="toggleDone" />
          Done
        </label>
        <span class="spacer"></span>
        <button v-if="threads" type="button" class="action" @click="replying = true">Reply</button>
        <button type="button" class="action" @click="editing = true">Edit</button>
        <button type="button" class="action" @click="emit('remove', comment.id)">Delete</button>
      </div>
    </template>
  </article>
</template>
