import type { Ui } from '@/api/types';

/// What the server already checked, checked again: these strings go into a
/// stylesheet of a page that holds the session token.
const TOKEN = /^[a-z][a-z0-9-]*$/;
const HEX = /^#([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/;

/// The mode of the page: `null` follows the system.
///
/// A theme of the user gives the mode of its base. A name the page does not
/// know, from a tab older than the last save, follows the system until the
/// next load.
export function mode(ui: Ui): 'light' | 'dark' | null {
  if (ui.theme === 'light' || ui.theme === 'dark') {
    return ui.theme;
  }
  const base = ui.themes?.[ui.theme]?.base;
  return base === 'light' || base === 'dark' ? base : null;
}

/// The rule of the theme in use, or nothing for a built-in one.
///
/// The selector beats both built-in palettes, (0,2,1) against (0,2,0).
/// Vite does not fix the order of the style elements it injects, so the
/// rule must win by its selector and not by its place.
export function sheet(ui: Ui): string {
  const theme = ui.themes?.[ui.theme];
  const base = mode(ui);
  if (!theme || !base) {
    return '';
  }
  const lines = Object.entries(theme.colors)
    .filter(([token, color]) => TOKEN.test(token) && HEX.test(color))
    .map(([token, color]) => `  --${token}: ${color};\n`);
  return `html:root[data-theme='${base}'] {\n${lines.join('')}}\n`;
}

/// Set the mode and the colors of the page.
export function applyTheme(ui: Ui | undefined) {
  const root = document.documentElement;
  const chosen = ui ? mode(ui) : null;
  if (chosen) {
    root.dataset.theme = chosen;
  } else {
    delete root.dataset.theme;
  }

  let style = document.getElementById('qreview-theme');
  if (!style) {
    style = document.createElement('style');
    style.id = 'qreview-theme';
    document.head.append(style);
  }
  style.textContent = ui ? sheet(ui) : '';
}
