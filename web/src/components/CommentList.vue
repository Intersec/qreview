<script setup lang="ts">
// What the session holds, as a list to walk.
//
// The changes come newest first, like the series pane above. Inside a change,
// the remarks keep the order of the export. The order never follows the change
// on the screen: a row that opens another change must not move the rows under
// the pointer. The list scrolls to that change instead.

import { computed, nextTick, ref, watch } from 'vue';
import { label } from '@/diff/paths';
import { open, previousGroups, rounds } from '@/diff/versions';
import type { ChangeComments, Comment } from '@/api/types';

const props = defineProps<{
  written: ChangeComments[];
  openKey: string | null;
  /// How tall the reader made it, in pixels.
  height: number;
}>();
const emit = defineEmits<{
  go: [key: string, remark: Comment];
}>();

const folded = ref(false);

/// The count is of the current remarks alone, like every other count on the
/// screen and like the export. The previous ones are listed under them, and
/// said to be previous.
const total = computed(() => props.written.reduce((sum, change) => sum + open(change).length, 0));

/// The pane stands as long as the session holds anything at all. A round
/// whose remarks are all previous counts nothing and must still be read.
const anything = computed(() => props.written.some((change) => change.comments.length > 0));

/// The changes newest first, as the export lists them oldest first. Inside
/// each one, the current remarks before the previous ones.
const groups = computed(() =>
  [...props.written].reverse().map((change) => ({
    change,
    current: rounds(change).current,
    previous: previousGroups(change),
  })),
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

function go(change: ChangeComments, comment: Comment) {
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
      <span>Comments · {{ total }}</span>
      <span aria-hidden="true">{{ folded ? '▸' : '▾' }}</span>
    </button>

    <div v-if="!folded" ref="body" class="list-body">
      <div
        v-for="group in groups"
        :key="group.change.key"
        class="list-group"
        :data-key="group.change.key"
      >
        <p class="list-change" :title="group.change.subject">
          <span v-if="group.change.key === openKey" class="tag">here</span>
          {{ group.change.subject }}
        </p>
        <button
          v-for="comment in group.current"
          :key="comment.id"
          type="button"
          class="row-button list-row"
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
        <template v-for="round in group.previous" :key="round.version.commit">
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
    </div>
  </section>
</template>
