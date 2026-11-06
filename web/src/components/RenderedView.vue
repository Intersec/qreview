<script setup lang="ts">
// A Markdown or AsciiDoc file, rendered by the server, beside its diff.
//
// The pane is read only: a remark is written in the diff. Each block of the
// page knows the lines it came from, which is how the pane and the diff
// follow each other as they scroll. See `roadmap/design.md` section 3.8.

import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { blockOf, lineWithin, linesOf, within, type Block } from '@/diff/blocks';
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
  /// The reader scrolled the pane: the diff can follow to this line.
  lead: [line: number];
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
/// Where the pane scrolled to follow the diff. That scroll is not the
/// reader's, and the diff must not follow it back.
let own: number | null = null;
let leading = false;

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

/// The height in the pane where the line of the diff stands.
const MARGIN = 12;

/// Bring a line of the new side to the top of the pane.
function follow(at: number) {
  line = at;
  const scroller = pane.value;
  if (!scroller || blocks.length === 0) {
    return;
  }
  const index = blockOf(blocks, at) ?? blocks.length - 1;
  const top = topOf(blocks[index], at);
  scroller.scrollTop += top - scroller.getBoundingClientRect().top - MARGIN;
  own = scroller.scrollTop;
}

function onScroll() {
  const scroller = pane.value;
  if (!scroller || blocks.length === 0) {
    return;
  }
  if (own !== null && Math.abs(scroller.scrollTop - own) < 1) {
    own = null;
    return;
  }
  if (leading) {
    return;
  }
  leading = true;
  requestAnimationFrame(() => {
    leading = false;
    lead();
  });
}

/// Tell the diff which line of the source stands at the top of the pane.
function lead() {
  const scroller = pane.value;
  if (!scroller || blocks.length === 0) {
    return;
  }
  const y = scroller.getBoundingClientRect().top + MARGIN;
  // The blocks stand in the order of the page, so the last one that starts
  // above the line is found by halves.
  let low = 0;
  let high = blocks.length - 1;
  while (low < high) {
    const middle = (low + high + 1) >> 1;
    if (blocks[middle].el.getBoundingClientRect().top <= y) {
      low = middle;
    } else {
      high = middle - 1;
    }
  }
  const block = blocks[low];
  // Below the bottom of a block is the gap before the next one, which
  // counts for the next block.
  const below = y > block.el.getBoundingClientRect().bottom;
  line = below ? (blocks[low + 1]?.from ?? block.to) : lineAt(block, y);
  emit('lead', line);
}

/// The rows of a code block.
///
/// The rows of a code block do not wrap, so each line of its source is one
/// row of the same height, and the place of a line is exact.
function rowsOf(block: Block & { el: HTMLElement }) {
  // A Markdown code block is the `<pre>`. An AsciiDoc one holds it, with
  // `data-code`, the line of its first row.
  const el = block.el.tagName === 'PRE' ? block.el : block.el.querySelector('pre[data-code]');
  if (!(el instanceof HTMLElement)) {
    return null;
  }
  const rows = (el.textContent ?? '').split('\n').length;
  // The lines of a fenced block count its fences, and the text does not.
  const fenced = block.to - block.from + 1 > rows ? block.from + 1 : block.from;
  const first = el.dataset.code ? Number(el.dataset.code) : fenced;
  const style = getComputedStyle(el);
  const above = parseFloat(style.paddingTop);
  const height = (el.clientHeight - above - parseFloat(style.paddingBottom)) / rows;
  const top = el.getBoundingClientRect().top + el.clientTop + above;
  return { first, rows, top, height };
}

/// Where a line of the source stands on the screen.
function topOf(block: Block & { el: HTMLElement }, at: number): number {
  const box = block.el.getBoundingClientRect();
  const code = rowsOf(block);
  if (!code) {
    return box.top + within(block, at) * box.height;
  }
  const row = Math.min(Math.max(at - code.first, 0), code.rows - 1);
  return row === 0 ? box.top : code.top + row * code.height;
}

/// The line of the source at a height on the screen. The inverse of `topOf`.
function lineAt(block: Block & { el: HTMLElement }, y: number): number {
  const box = block.el.getBoundingClientRect();
  const code = rowsOf(block);
  if (!code) {
    return box.height > 0 ? lineWithin(block, (y - box.top) / box.height) : block.from;
  }
  const row = Math.floor((y - code.top) / code.height);
  return code.first + Math.min(Math.max(row, 0), code.rows - 1);
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
  <aside ref="pane" class="rendered-pane" aria-label="The file, rendered" @scroll="onScroll">
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
