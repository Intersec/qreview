<script setup lang="ts">
// A thread already posted on Gerrit: the remark, the replies that Gerrit
// holds under it, and the replies written here. Gerrit owns its comments,
// so they have no edit and no delete. A reply written here stays here,
// unless it is For Gerrit: then it goes out in the thread at the next
// Send. See section 6.8 of the design.

import { computed, inject, ref, watch } from 'vue';
import CommentBox from './CommentBox.vue';
import { render } from '@/diff/markdown';
import { read, threadSlot } from '@/diff/drafts';
import { GERRIT_PARENT, THREADS, isResolved, resolvesHere } from '@/diff/threads';
import type { Comment, PostedComment } from '@/api/types';

const props = defineProps<{
  comment: PostedComment;
  /// Set when the line it spoke of is not in this version.
  stranded?: boolean;
}>();

const threads = inject(THREADS, null);

const replies = computed(() => threads?.postedRepliesOf?.(props.comment.id) ?? []);
const ours = computed(() => threads?.repliesOf(`${GERRIT_PARENT}${props.comment.id}`) ?? []);
/// Over ssh, Gerrit gives no id: nothing can answer the thread.
const answerable = computed(() => props.comment.unresolved !== null && threads !== null);
const resolved = computed(() => isResolved(props.comment, replies.value));
/// Resolved on Gerrit, or by a reply written here.
const done = computed(() => resolved.value || resolvesHere(ours.value));
/// A resolving reply that left is a draft on Gerrit: it is taken back there.
const doneSent = computed(() =>
  ours.value.some((reply) => reply.gerrit?.resolve && reply.gerrit.state === 'sent'),
);

/// Null until the reader folds or unfolds it, so the state decides.
const foldedByHand = ref<boolean | null>(null);
const folded = computed(() => foldedByHand.value ?? done.value);
watch(done, () => (foldedByHand.value = null));

const replying = ref(read(threadSlot(`${GERRIT_PARENT}${props.comment.id}`, 'reply')) !== '');

/// The place it was posted on, which this version no longer has.
const place = computed(() =>
  props.comment.line === null ? props.comment.file : `${props.comment.file}:${props.comment.line}`,
);

function firstLine(text: string): string {
  return text.split('\n', 1)[0] ?? '';
}

function who(reply: Comment): string {
  return reply.author === 'agent' ? 'Agent' : 'You';
}

async function sendReply(body: string, _ask: boolean, gerrit: boolean, resolve: boolean) {
  replying.value = false;
  await threads?.replyOnGerrit?.(props.comment.id, body, gerrit ? { resolve } : null);
}

async function toggleDone(event: Event) {
  await threads?.setGerritDone?.(props.comment.id, (event.target as HTMLInputElement).checked);
}
</script>

<template>
  <article
    class="talk-box posted-box"
    :class="[
      stranded ? 'talk-stranded' : '',
      done ? 'talk-done' : '',
      done && folded ? 'talk-folded' : '',
    ]"
  >
    <p class="talk-head">
      <button
        v-if="done"
        type="button"
        class="talk-fold"
        :aria-expanded="!folded"
        :title="folded ? 'Show this resolved thread' : 'Fold this resolved thread'"
        @click="foldedByHand = !folded"
      >
        <span>{{ folded ? '▸' : '▾' }}</span>
        <span class="talk-who">{{ comment.author }}</span>
        <span class="talk-tag">Gerrit</span>
        <span class="talk-tag">{{ resolved ? 'resolved' : 'done' }}</span>
        <span v-if="folded" class="talk-summary">{{ firstLine(comment.body) }}</span>
        <span v-if="folded && replies.length + ours.length" class="talk-summary"
          >· {{ replies.length + ours.length }}
          {{ replies.length + ours.length === 1 ? 'reply' : 'replies' }}</span
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
      <div
        v-for="reply in ours"
        :key="reply.id"
        class="talk-reply"
        :class="reply.author === 'agent' ? 'talk-reply-agent' : ''"
      >
        <p class="talk-reply-head">
          <span class="talk-who">{{ who(reply) }}</span>
          <span
            v-if="reply.gerrit?.state === 'staged'"
            class="talk-tag talk-tag-gerrit"
            title="This reply goes out in the thread when you click Send"
            >for Gerrit</span
          >
          <span
            v-else-if="reply.gerrit?.state === 'sent'"
            class="talk-tag talk-tag-gerrit"
            title="This reply is a draft on Gerrit. Edit or publish it there"
            >draft on Gerrit</span
          >
          <span v-if="reply.gerrit?.resolve" class="talk-tag">resolves</span>
          <template v-if="reply.gerrit?.state !== 'sent'">
            <span class="spacer"></span>
            <button type="button" class="action" @click="threads?.remove?.(reply.id)">
              Delete
            </button>
          </template>
        </p>
        <!-- eslint-disable-next-line vue/no-v-html -- sanitized in diff/markdown.ts -->
        <div class="prose-comment" v-html="render(reply.body)"></div>
      </div>

      <div v-if="replying" class="talk-body">
        <CommentBox
          label="Reply"
          :draft="threadSlot(`${GERRIT_PARENT}${comment.id}`, 'reply')"
          gerrit-reply
          @save="sendReply"
          @cancel="replying = false"
        />
      </div>

      <div v-if="answerable && !replying" class="talk-foot">
        <label
          class="talk-done-box"
          title="Stage a reply Done that resolves this thread on Gerrit, as its Done button does"
        >
          <input
            type="checkbox"
            :checked="done"
            :disabled="resolved || doneSent"
            @change="toggleDone"
          />
          Done
        </label>
        <span class="spacer"></span>
        <button type="button" class="action" @click="replying = true">Reply</button>
      </div>
    </template>
  </article>
</template>
