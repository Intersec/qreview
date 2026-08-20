// Comment bodies are Markdown.
//
// The text is written by the person at the keyboard, but it is still rendered
// as HTML, so it goes through a sanitizer. A comment must never be able to
// run anything.

import DOMPurify from 'dompurify';
import { marked } from 'marked';

marked.setOptions({ breaks: true, gfm: true });

export function render(body: string): string {
  const html = marked.parse(body, { async: false });

  return DOMPurify.sanitize(html, { USE_PROFILES: { html: true } });
}
