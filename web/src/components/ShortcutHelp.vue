<script setup lang="ts">
defineEmits<{ close: [] }>();

/// The same keys Gerrit uses, so a reader who lives in Gerrit needs no
/// second set of habits.
const KEYS: [string, string][] = [
  ['j / k', 'The next or the previous line'],
  ['n / p', 'The next or the previous hunk'],
  ['] / [', 'The next or the previous file'],
  ['J / K', 'The next or the previous change'],
  ['c', 'Write on the line the keyboard is on'],
  ['u', 'Show or hide the series'],
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
