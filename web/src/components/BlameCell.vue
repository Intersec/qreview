<script setup lang="ts">
import { computed } from 'vue';
import { label, type BlameLine, type BlameTarget } from '@/diff/blame';
import { format } from '@/place';

const props = defineProps<{
  /// The line of the file on the left, or null on a row that has none.
  line: number | null;
  lines: Map<number, BlameLine>;
  target: (commit: string) => BlameTarget | null;
  /// The whole blame is on its way, so a line with no blame yet may get one.
  waiting?: boolean;
}>();
const emit = defineEmits<{
  open: [at: BlameLine];
  peek: [at: BlameLine, cell: HTMLElement];
  leave: [];
}>();

const at = computed(() => (props.line === null ? undefined : props.lines.get(props.line)));
const goes = computed(() => (at.value ? props.target(at.value.commit.commit) : null));

/// A change of the series has an address in the review too, so the menu of
/// the browser can open it in a new tab. That tab lands on the file.
const href = computed(() => {
  if (!at.value || !goes.value) {
    return null;
  }
  const place = format({ change: goes.value.key, file: at.value.commit.path });

  return `${location.pathname}${location.search}${place}`;
});

/// A plain click stays in this page and lands on the line. With a modifier,
/// the click is the browser's: a new tab or a new window.
function follow(event: MouseEvent) {
  event.stopPropagation();
  const plain = !(event.ctrlKey || event.metaKey || event.shiftKey || event.altKey);
  if (plain && at.value) {
    event.preventDefault();
    emit('open', at.value);
  }
}
</script>

<template>
  <td
    class="blame"
    @mouseenter="at && emit('peek', at, $event.currentTarget as HTMLElement)"
    @mouseleave="emit('leave')"
  >
    <template v-if="at">
      <!-- A link, so the browser shows where it goes, as Gerrit does. -->
      <a v-if="href" class="blame-label blame-go" :href="href" @click="follow">{{
        label(at.commit)
      }}</a>
      <span v-else class="blame-label">{{ label(at.commit) }}</span>
    </template>
    <span v-else-if="waiting && line !== null" class="blame-label blame-wait">…</span>
  </td>
</template>
