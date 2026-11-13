<script setup lang="ts">
import { computed, inject, onMounted, ref, watch } from 'vue';
import { drop, read, write } from '@/diff/drafts';
import { THREADS } from '@/diff/threads';

const props = defineProps<{
  start?: string;
  label: string;
  /// Where an unfinished remark is kept. Without it, nothing is kept.
  draft?: string;
  /// The box can save and ask the agent to answer in one go.
  askable?: boolean;
  /// The box writes a new remark, which can go to Gerrit.
  remark?: boolean;
  /// The box replies to a thread of Gerrit: the reply can go out in it, and
  /// resolve it.
  gerritReply?: boolean;
}>();
const emit = defineEmits<{
  save: [body: string, ask: boolean, gerrit: boolean, resolve: boolean];
  cancel: [];
}>();

/// The For Gerrit box, on a new remark on a version that Gerrit holds, or
/// on a reply to a thread of Gerrit, which always has its patch set there.
const threads = inject(THREADS, null);
const offersGerrit = computed(
  () => props.gerritReply === true || (props.remark === true && threads?.onGerrit?.value === true),
);
const forGerrit = ref(false);
const resolve = ref(false);

/// What the reader typed and did not save wins over the text the box starts
/// from: an edit that a reload interrupted comes back as it was left.
function initial(): string {
  const kept = props.draft ? read(props.draft) : '';
  return kept !== '' ? kept : (props.start ?? '');
}

const body = ref(initial());
const area = ref<HTMLTextAreaElement | null>(null);

// A box that opens is a box the reader asked for, so the keyboard goes into
// it, and the browser scrolls it into view. A box that comes back with an unfinished remark does not take the
// keyboard: the reader opened a file, not that box.
onMounted(() => {
  // An edit is asked for too, and its box may open far from the click.
  if (body.value === '' || body.value === props.start) {
    area.value?.focus();
    const end = body.value.length;
    area.value?.setSelectionRange(end, end);
  }
});

// A new version of the change moved the draft under another place.
watch(
  () => props.draft,
  () => {
    body.value = initial();
  },
);

// Every key stroke, so that opening another file costs nothing. The store
// is the browser's own: an unfinished remark is not a comment.
watch(body, (text) => {
  // An edit brought back to the text it started from is not a draft.
  if (props.draft) {
    write(props.draft, text === props.start ? '' : text);
  }
});

function save(ask = false) {
  if (body.value.trim() === '') {
    return;
  }
  forget();
  const gerrit = offersGerrit.value && forGerrit.value;
  emit('save', body.value, ask && props.askable === true, gerrit, gerrit && resolve.value);
}

function forget() {
  if (props.draft) {
    drop(props.draft);
  }
}
</script>

<template>
  <form class="talk-box" @submit.prevent="save()">
    <p class="talk-head">{{ label }}</p>

    <div class="talk-body">
      <label class="sr-only" :for="`box-${label}`">{{ label }}</label>
      <!-- Ctrl+S is the browser's own save, so this one is prevented. -->
      <textarea
        :id="`box-${label}`"
        ref="area"
        v-model="body"
        class="talk-text"
        :placeholder="label"
        @keydown.ctrl.enter.exact="save()"
        @keydown.ctrl.shift.enter.exact="save(true)"
        @keydown.ctrl.s.prevent="save()"
      ></textarea>
    </div>

    <div class="talk-foot">
      <span class="talk-hint"
        >Markdown. Ctrl+Enter or Ctrl+S saves<template v-if="askable"
          >, Ctrl+Shift+Enter saves and asks the agent</template
        >.</span
      >
      <span class="spacer"></span>
      <label
        v-if="offersGerrit"
        class="talk-done-box"
        title="Send this remark to Gerrit as a draft, when you click Send"
      >
        <input v-model="forGerrit" type="checkbox" />
        For Gerrit
      </label>
      <label
        v-if="gerritReply && forGerrit"
        class="talk-done-box"
        title="Resolve the thread on Gerrit with this reply"
      >
        <input v-model="resolve" type="checkbox" />
        Resolve
      </label>
      <button
        type="button"
        class="action"
        @click="
          forget();
          emit('cancel');
        "
      >
        Cancel
      </button>
      <button
        v-if="askable"
        type="button"
        class="action"
        :disabled="body.trim() === ''"
        title="Save, and ask the agent to answer (Ctrl+Shift+Enter)"
        @click="save(true)"
      >
        Save and ask
      </button>
      <button type="submit" class="action" :disabled="body.trim() === ''">Save</button>
    </div>
  </form>
</template>
