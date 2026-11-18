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

  it('sends the theme the reader picked', async () => {
    const wrapper = mount(PreferencesDialog, { props: { config: config('system') } });

    await wrapper.findAll('select')[2].setValue('dark');
    await wrapper.find('form').trigger('submit');

    expect(saved(wrapper).ui).toEqual({ diff: 'unified', theme: 'dark' });
  });
});
