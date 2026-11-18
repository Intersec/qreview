// @vitest-environment jsdom
//
// The panel writes the file of the user. It must write only what the
// reader changed in it.

import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import PreferencesDialog from './PreferencesDialog.vue';
import type { Config } from '@/api/types';

function config(theme: string): Config {
  return {
    languages: {},
    gerrit: { enabled: false, branch: null },
    series: { maxCommits: 50, guessMax: 10, batchSize: 5, integrationBranch: null },
    ui: {
      diff: 'unified',
      theme,
      themes: { night: { base: 'dark', colors: {} } },
    },
    diff: {
      context: 10,
      wrap: false,
      ignoreWhitespace: false,
      tabWidth: 4,
      fontSize: 12,
      syntax: true,
    },
  };
}

/// The select of the theme, found by its label and not by its place.
function themeSelect(wrapper: ReturnType<typeof mount>) {
  const label = wrapper.findAll('label').find((l) => l.text().startsWith('Theme'));
  return label!.find('select');
}

function saved(wrapper: ReturnType<typeof mount>): { ui: Record<string, string> } {
  const events = wrapper.emitted('save') as [{ ui: Record<string, string> }][];
  return events[0][0];
}

describe('the preferences panel', () => {
  it('sends no theme when the reader did not change it', async () => {
    // The page shows system because the theme of the file did not load.
    const wrapper = mount(PreferencesDialog, { props: { config: config('system') } });

    await wrapper.find('form').trigger('submit');

    expect(saved(wrapper).ui).toEqual({ diff: 'unified' });
  });

  it('offers the themes of the user after the built-in ones', () => {
    const wrapper = mount(PreferencesDialog, { props: { config: config('system') } });

    const names = themeSelect(wrapper)
      .findAll('option')
      .map((o) => o.element.value);

    expect(names).toEqual(['system', 'light', 'dark', 'night']);
  });

  it('sends the theme the reader picked', async () => {
    const wrapper = mount(PreferencesDialog, { props: { config: config('system') } });

    await themeSelect(wrapper).setValue('dark');
    await wrapper.find('form').trigger('submit');

    expect(saved(wrapper).ui).toEqual({ diff: 'unified', theme: 'dark' });
  });

  it('sends a picked theme even when it is the one shown', async () => {
    // The file names a theme that did not load, so the page shows system.
    // Picking system on purpose must clear the name from the file.
    const wrapper = mount(PreferencesDialog, { props: { config: config('system') } });

    await themeSelect(wrapper).setValue('system');
    await wrapper.find('form').trigger('submit');

    expect(saved(wrapper).ui).toEqual({ diff: 'unified', theme: 'system' });
  });
});
