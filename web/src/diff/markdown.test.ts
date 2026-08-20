// @vitest-environment jsdom
// DOMPurify needs a DOM. The browser has one; a node test has to ask.
import { describe, expect, it } from 'vitest';
import { render } from './markdown';

describe('render', () => {
  it('turns markdown into html', () => {
    expect(render('a **bold** word')).toContain('<strong>bold</strong>');
  });

  it('keeps a code span readable', () => {
    expect(render('call `retry()` twice')).toContain('<code>retry()</code>');
  });

  it('treats a single newline as a line break', () => {
    expect(render('one\ntwo')).toContain('<br>');
  });

  it('strips a script', () => {
    const html = render('before <script>alert(1)</script> after');
    expect(html).not.toContain('<script');
    expect(html).toContain('before');
  });

  it('strips an event handler', () => {
    expect(render('<img src=x onerror="alert(1)">')).not.toContain('onerror');
  });
});
