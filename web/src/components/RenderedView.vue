<script setup lang="ts">
// A Markdown file, rendered by the server, beside its diff.
//
// The pane is read only: a remark is written in the diff. Each block of the
// page knows the lines it came from, which is how the pane follows the diff
// as it scrolls. See `roadmap/design.md` section 3.8.

import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { blockOf, linesOf, type Block } from '@/diff/blocks';
import { clean } from '@/diff/markdown';
import type { FileDiff } from '@/api/types';

const props = defineProps<{
  diff: FileDiff;
  load: () => Promise<string>;
  /// Where an image of the tree is fetched, by its path.
  imageUrl: (path: string) => string;
}>();
const emit = defineEmits<{
  /// The page is on the screen: the diff can say where to stand.
  ready: [];
  /// The reader asked to see removed lines, in the diff.
  reveal: [side: 'old', line: number];
}>();

const html = ref('');
const failed = ref<string | null>(null);
const loading = ref(false);
const pane = ref<HTMLElement | null>(null);
const host = ref<HTMLElement | null>(null);

/// A mark in the gutter: a bar beside a block the change touches, or a tick
/// where lines went away.
interface Mark {
  top: number;
  height: number;
  change?: string;
  old?: string;
  title?: string;
}
const marks = ref<Mark[]>([]);

/// The leaf blocks on the page, in the order they are read.
let blocks: (Block & { el: HTMLElement })[] = [];
/// The line the diff stands on, kept so a page that arrives late lands there.
let line = 1;

async function read() {
  loading.value = true;
  failed.value = null;
  try {
    html.value = clean(await props.load());
  } catch (e) {
    failed.value = e instanceof Error ? e.message : String(e);
    html.value = '';
  } finally {
    loading.value = false;
  }
  await nextTick();
  // The server names an image by its path in the tree, and only the page
  // knows the change and the version it is read from. An image that arrives
  // moves the blocks under it, so the pane finds its place again.
  for (const img of host.value?.querySelectorAll<HTMLImageElement>('img[data-path]') ?? []) {
    img.addEventListener('load', () => follow(line), { once: true });
    img.src = props.imageUrl(img.dataset.path ?? '');
  }
  blocks = [];
  for (const el of host.value?.querySelectorAll<HTMLElement>('[data-lines]') ?? []) {
    const lines = linesOf(el.dataset.lines);
    if (lines) {
      blocks.push({ ...lines, el });
    }
  }
  place();
  follow(line);
  emit('ready');
}

/// Put the marks beside their blocks.
///
/// The marks stand in a gutter of their own rather than on the blocks, so a
/// nested item or a table row is marked in the same column as a paragraph.
function place() {
  const page = host.value;
  if (!page) {
    marks.value = [];
    return;
  }
  const origin = page.getBoundingClientRect().top;
  const found: Mark[] = [];
  for (const el of page.querySelectorAll<HTMLElement>('[data-change], .md-removed')) {
    const box = el.getBoundingClientRect();
    found.push({
      top: box.top - origin,
      height: box.height,
      change: el.dataset.change,
      old: el.classList.contains('md-removed') ? el.dataset.old : undefined,
      title: el.title || undefined,
    });
  }
  marks.value = found;
}

// The text wraps again when the pane changes width, and an image moves the
// blocks under it: both change the height of the page.
const resized = new ResizeObserver(() => place());
watch(host, (el, old) => {
  if (old) {
    resized.unobserve(old);
  }
  if (el) {
    resized.observe(el);
  }
});
onBeforeUnmount(() => resized.disconnect());

onMounted(read);
// Another version of the file, or another file, is another page.
watch(() => props.diff, read);

/// Bring the block of a line of the new side to the top of the pane.
function follow(at: number) {
  line = at;
  const scroller = pane.value;
  if (!scroller || blocks.length === 0) {
    return;
  }
  const index = blockOf(blocks, at) ?? blocks.length - 1;
  const top = blocks[index].el.getBoundingClientRect().top;
  scroller.scrollTop += top - scroller.getBoundingClientRect().top - 12;
}

/// A tick where lines went away brings the diff to them.
function reveal(old: string | undefined) {
  const lines = linesOf(old);
  if (lines) {
    emit('reveal', 'old', lines.from);
  }
}

defineExpose({ follow });
</script>

<template>
  <aside ref="pane" class="rendered-pane" aria-label="The file, rendered">
    <p v-if="failed" class="note warn">{{ failed }}</p>
    <p v-else-if="loading && html === ''" class="note">Rendering the file…</p>
    <div class="md-page">
      <div class="md-gutter">
        <template v-for="(mark, i) in marks" :key="i">
          <button
            v-if="mark.old"
            type="button"
            class="md-tick"
            :style="{ top: `${mark.top}px` }"
            :title="mark.title"
            :aria-label="mark.title"
            @click="reveal(mark.old)"
          />
          <span
            v-else
            class="md-bar"
            :data-change="mark.change"
            :style="{ top: `${mark.top}px`, height: `${mark.height}px` }"
          />
        </template>
      </div>
      <!-- eslint-disable-next-line vue/no-v-html -- written by the server, sanitized in diff/markdown.ts -->
      <article ref="host" class="md-doc" v-html="html" />
    </div>
  </aside>
</template>
