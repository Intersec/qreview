import type { Ui } from '@/api/types';

/// The mode of the page: `null` follows the system.
///
/// A theme of the user gives the mode of its base. A name the page does not
/// know, from a tab older than the last save, follows the system until the
/// next load.
export function mode(ui: Ui): 'light' | 'dark' | null {
  if (ui.theme === 'light' || ui.theme === 'dark') {
    return ui.theme;
  }
  return ui.themes?.[ui.theme]?.base ?? null;
}
