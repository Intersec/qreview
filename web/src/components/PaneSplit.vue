<script setup lang="ts">
// The bar between two panes, dragged to give one of them more room.
//
// It reports how far it moved and lets the parent decide what that means.
// The parent owns the size, because the parent is the one that knows what
// the smallest useful pane is.

import { onBeforeUnmount, ref } from 'vue';

const props = defineProps<{
  /// `vertical` is a bar between two columns, dragged left and right.
  /// `horizontal` is a bar between two rows, dragged up and down.
  direction: 'vertical' | 'horizontal';
  label: string;
}>();
const emit = defineEmits<{ move: [by: number]; done: [] }>();

const dragging = ref(false);
let from = 0;

function down(event: PointerEvent) {
  dragging.value = true;
  from = props.direction === 'vertical' ? event.clientX : event.clientY;
  window.addEventListener('pointermove', move);
  window.addEventListener('pointerup', up);
  // The pointer leaves the bar as soon as it moves, and the text under it
  // would be selected on the way.
  event.preventDefault();
}

function move(event: PointerEvent) {
  const now = props.direction === 'vertical' ? event.clientX : event.clientY;
  emit('move', now - from);
  from = now;
}

function up() {
  dragging.value = false;
  window.removeEventListener('pointermove', move);
  window.removeEventListener('pointerup', up);
  emit('done');
}

/// The keyboard moves it too, which is what a separator is expected to do.
function key(event: KeyboardEvent) {
  const back = props.direction === 'vertical' ? 'ArrowLeft' : 'ArrowUp';
  const on = props.direction === 'vertical' ? 'ArrowRight' : 'ArrowDown';
  const step = event.shiftKey ? 40 : 8;

  if (event.key !== back && event.key !== on) {
    return;
  }
  event.preventDefault();
  emit('move', event.key === back ? -step : step);
  emit('done');
}

onBeforeUnmount(up);
</script>

<template>
  <div
    class="split"
    :class="[`split-${direction}`, dragging ? 'is-dragging' : '']"
    role="separator"
    tabindex="0"
    :aria-orientation="direction === 'vertical' ? 'vertical' : 'horizontal'"
    :aria-label="label"
    @pointerdown="down"
    @keydown="key"
  ></div>
</template>
