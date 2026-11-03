<script setup lang="ts">
defineEmits<{ close: [] }>();

/// The same keys Gerrit uses, so a reader who lives in Gerrit needs no
/// second set of habits.
const KEYS: [string, string][] = [
  ['j / k', 'The next or the previous line'],
  ['n / p', 'The next or the previous hunk'],
  ['X', 'Open every folded line of the file, or fold them back'],
  ['] / [', 'The next or the previous file'],
  ['J / K', 'The next or the previous change'],
  ['v', 'Start a range on this line. j and k grow it'],
  ['c', 'Write on the line, or on the range, or on what the mouse selected'],
  ['a / A', 'Ask the agent to fix this change, or the whole series'],
  [
    'Ctrl+Shift+Enter',
    'Ask the agent about the thread on this line. In a comment box: save, then ask',
  ],
  ['u', 'Show or hide the series'],
  ['b', 'Show or hide the blame of the left side'],
  ['m', 'Switch between the unified and the side by side views'],
  ['/', 'Filter the files'],
  [', ', 'Preferences'],
  ['?', 'This list'],
];
</script>

<template>
  <div class="veil" @click.self="$emit('close')">
    <div class="panel" role="dialog" aria-modal="true" aria-label="Keyboard shortcuts">
      <h2 class="panel-title">Keyboard shortcuts</h2>

      <p v-for="[key, what] in KEYS" :key="key" class="row">
        <kbd>{{ key.trim() }}</kbd>
        <span>{{ what }}</span>
      </p>

      <div class="panel-foot">
        <button type="button" class="action" @click="$emit('close')">Close</button>
      </div>
    </div>
  </div>
</template>
