<script setup lang="ts">
// The comments to handle, as a list to walk: the threads of reviewers that
// wait, then the remarks of the review that are still open.
//
// The changes come newest first, like the series pane above. Inside a change,
// the remarks keep the order of the export. The order never follows the change
// on the screen: a row that opens another change must not move the rows under
// the pointer. The list scrolls to that change instead.

import { computed, nextTick, ref, watch } from 'vue';
import { label } from '@/diff/paths';
import { previousGroups, rounds, toHandle } from '@/diff/versions';
import type { ChangeComments, Comment } from '@/api/types';

const props = defineProps<{
  written: ChangeComments[];
  /// The threads of reviewers on Gerrit that wait, change by change. They
  /// come first: a reviewer waits on them.
  gerrit: { key: string; subject: string; threads: Comment[] }[];
  /// True while the threads of Gerrit are read: more may come.
  gerritReading?: boolean;
  openKey: string | null;
  /// How tall the reader made it, in pixels.
  height: number;
}>();
const emit = defineEmits<{
  go: [key: string, remark: Comment];
}>();

const folded = ref(false);
/// The done, sent and previous remarks stay a click away: a remark is
/// never hidden, but the list is of what is left to handle.
const showAll = ref(false);

/// The pane stands as long as the session holds anything at all. A round
/// whose remarks are all previous counts nothing and must still be read.
const anything = computed(
  () =>
    props.gerritReading === true ||
    props.gerrit.length > 0 ||
    props.written.some((change) => change.comments.length > 0),
);

/// The changes newest first, as the export lists them oldest first. Inside
/// each one, the open remarks, then on request the done and previous ones.
const groups = computed(() =>
  [...props.written].reverse().map((change) => {
    const open = toHandle(change);
    return {
      change,
      open,
      done: rounds(change).current.filter((comment) => !open.includes(comment)),
      previous: previousGroups(change),
    };
  }),
);

/// The count is of the rows to handle.
const total = computed(
  () =>
    groups.value.reduce((sum, group) => sum + group.open.length, 0) +
    props.gerrit.reduce((sum, change) => sum + change.threads.length, 0),
);

/// How many remarks the list keeps folded.
const hidden = computed(() =>
  groups.value.reduce(
    (sum, group) =>
      sum +
      group.done.length +
      group.previous.reduce((count, round) => count + round.comments.length, 0),
    0,
  ),
);

const body = ref<HTMLElement | null>(null);

/// Bring the remarks of the change on the screen into view. `nearest` leaves
/// the list still when they are already in sight, as after a click on a row.
watch(
  () => props.openKey,
  async (key) => {
    await nextTick();
    const group = [...(body.value?.querySelectorAll<HTMLElement>('.list-group') ?? [])].find(
      (element) => element.dataset.key === key,
    );
    group?.scrollIntoView({ block: 'nearest' });
  },
  { immediate: true },
);

/// Where a comment sits, short enough for a narrow pane.
function place(comment: Comment): string {
  const anchor = comment.anchor;
  if (!anchor) {
    return 'the change';
  }
  const cut = anchor.file.lastIndexOf('/');
  const name = anchor.file.startsWith('/') ? label(anchor.file) : anchor.file.slice(cut + 1);
  if (anchor.startLine === null) {
    return name;
  }
  const end = anchor.endLine ?? anchor.startLine;

  return end > anchor.startLine
    ? `${name}:${anchor.startLine}-${end}`
    : `${name}:${anchor.startLine}`;
}

/// The first line of the body, which is what a list has room for.
function gist(comment: Comment): string {
  return comment.body.split('\n').find((line) => line.trim() !== '') ?? '';
}

function go(change: { key: string }, comment: Comment) {
  emit('go', change.key, comment);
}
</script>

<template>
  <section
    v-if="anything"
    class="comment-list"
    :class="folded ? 'is-folded' : ''"
    :style="folded ? undefined : { height: `${height}px` }"
  >
    <button
      type="button"
      class="pane-title list-head"
      :aria-expanded="!folded"
      @click="folded = !folded"
    >
      <span>To handle · {{ total }}</span>
      <span aria-hidden="true">{{ folded ? '▸' : '▾' }}</span>
    </button>

    <div v-if="!folded" ref="body" class="list-body">
      <p v-if="gerritReading" class="list-reading" role="status">Reading the threads of Gerrit…</p>
      <!-- The reviewers wait on these: they come before anything else. -->
      <div
        v-for="change in gerrit"
        :key="`gerrit-${change.key}`"
        class="list-group is-gerrit"
        :data-key="change.key"
      >
        <p class="list-change" :title="change.subject">
          <span class="tag tag-gerrit">Gerrit</span>
          <span v-if="change.key === openKey" class="tag">here</span>
          {{ change.subject }}
        </p>
        <button
          v-for="thread in change.threads"
          :key="thread.id"
          type="button"
          class="row-button list-row"
          :title="`${thread.anchor?.file ?? 'the change'} — ${gist(thread)}`"
          @click="go(change, thread)"
        >
          <code class="list-place">{{ place(thread) }}</code>
          <span class="list-gist">{{ gist(thread) }}</span>
        </button>
      </div>

      <template v-for="group in groups" :key="group.change.key">
        <div
          v-if="
            group.open.length > 0 ||
            (showAll && (group.done.length > 0 || group.previous.length > 0))
          "
          class="list-group"
          :data-key="group.change.key"
        >
          <p class="list-change" :title="group.change.subject">
            <span v-if="group.change.key === openKey" class="tag">here</span>
            {{ group.change.subject }}
          </p>
          <button
            v-for="comment in showAll ? [...group.open, ...group.done] : group.open"
            :key="comment.id"
            type="button"
            class="row-button list-row"
            :class="group.done.includes(comment) ? 'is-done' : ''"
            :title="`${comment.anchor?.file ?? 'the change'} — ${gist(comment)}`"
            @click="go(group.change, comment)"
          >
            <code class="list-place">{{ place(comment) }}</code>
            <span class="list-gist">{{ gist(comment) }}</span>
          </button>

          <!-- The previous remarks: written on a version the change no longer
             carries, under the version each one belongs to. They are counted
             nowhere and exported nowhere, and they are here because a remark
             is never hidden. -->
          <template v-for="round in showAll ? group.previous : []" :key="round.version.commit">
            <p class="list-previous" :title="round.version.subject">
              <span class="tag">previous</span>
              <code>{{ round.version.commit.slice(0, 8) }}</code>
              <span class="list-subject">{{ round.version.subject }}</span>
            </p>
            <button
              v-for="comment in round.comments"
              :key="comment.id"
              type="button"
              class="row-button list-row is-previous"
              :title="`${comment.anchor?.file ?? 'the change'} — ${gist(comment)}`"
              @click="go(group.change, comment)"
            >
              <code class="list-place">{{ place(comment) }}</code>
              <span class="list-gist">{{ gist(comment) }}</span>
            </button>
          </template>
        </div>
      </template>

      <button
        v-if="hidden > 0"
        type="button"
        class="row-button list-more"
        @click="showAll = !showAll"
      >
        {{ showAll ? 'Hide the handled remarks' : `Show the handled remarks · ${hidden}` }}
      </button>
    </div>
  </section>
</template>
