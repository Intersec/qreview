// The word the pointer is on, and where else it stands.
//
// Reading a change means following a name: where does this variable come
// from, and who else reads it. The pointer answers that without a click.
//
// The two functions are the whole of it, and they are pure: one reads a
// word out of a line, the other finds that word again in another line. The
// injection key sits here too, because the rows that paint the word and the
// pane that reads it both need one name for it.

import type { InjectionKey, Ref } from 'vue';

/** A run of a line, in UTF-16 units, the way the wire counts them. */
export interface Run {
  start: number;
  end: number;
}

/// The word the pointer is on, shared with every row of the diff.
///
/// Provided rather than passed: a prop would render the whole table again
/// on every move of the pointer, and only the rows that carry the word have
/// anything new to paint.
export const HOVERED: InjectionKey<Ref<string | null>> = Symbol('hovered word');

/// What a name is made of. A dot and a dash cut one, so `a.b` is two names
/// and `foo-bar` is two names, which is what CSS and code both mean.
const PART = /[\p{L}\p{N}_$]/u;

/// A run of digits is not a name. Lighting up every `0` of a file would say
/// nothing at all.
const NUMBER = /^\p{N}+$/u;

/// The word that covers a place in the line, or null when none does.
///
/// `at` is a caret, so it stands between two characters. The word before it
/// counts as much as the word after: a pointer on the last letter of a name
/// means that name.
export function wordAt(text: string, at: number): string | null {
  const index = at < text.length && PART.test(text[at]) ? at : at - 1;
  if (index < 0 || index >= text.length || !PART.test(text[index])) {
    return null;
  }

  let start = index;
  while (start > 0 && PART.test(text[start - 1])) {
    start -= 1;
  }
  let end = index + 1;
  while (end < text.length && PART.test(text[end])) {
    end += 1;
  }

  const word = text.slice(start, end);

  return NUMBER.test(word) ? null : word;
}

/// Every place the word stands in the line, whole and on its own.
///
/// `fd` is not the `fd` of `fdesc`: a reader following a variable does not
/// want every name that happens to contain its letters.
export function occurrences(text: string, word: string): Run[] {
  const out: Run[] = [];
  if (word === '') {
    return out;
  }

  let start = text.indexOf(word);
  while (start >= 0) {
    const end = start + word.length;
    const alone =
      (start === 0 || !PART.test(text[start - 1])) &&
      (end === text.length || !PART.test(text[end]));
    if (alone) {
      out.push({ start, end });
    }
    // One past the start, not past the end: a match that is refused can
    // hold the first letters of one that is not.
    start = text.indexOf(word, start + 1);
  }

  return out;
}
