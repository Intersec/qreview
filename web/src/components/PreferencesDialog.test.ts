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
      themes: {
        midnight: { base: 'dark', colors: {}, bundled: false },
        nord: { base: 'dark', colors: {}, bundled: true },
        dracula: { base: 'dark', colors: {}, bundled: true },
      },
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

interface Patch {
  diff?: Record<string, unknown>;
  ui?: Record<string, string>;
}

function saved(wrapper: ReturnType<typeof mount>): Patch {
  const events = wrapper.emitted('save') as [Patch][];
  return events[0][0];
}

/// The control of a row, found by the label of the row.
function control(wrapper: ReturnType<typeof mount>, label: string) {
  const row = wrapper.findAll('label').find((l) => l.text().startsWith(label));
  return row!.find('select, input');
}

describe('the preferences panel', () => {
  it('sends nothing that the reader did not touch', async () => {
    // A value shown may come from the .qreview.json of the repository, or
    // be a fallback. A save must not copy it into the file of the user.
    const wrapper = mount(PreferencesDialog, { props: { config: config('system') } });

    await wrapper.find('form').trigger('submit');

    expect(saved(wrapper)).toEqual({});
  });

  it('sends only the fields that the reader touched', async () => {
    const wrapper = mount(PreferencesDialog, { props: { config: config('system') } });

    await control(wrapper, 'Context').setValue(25);
    await control(wrapper, 'Wrap').setValue(true);
    await control(wrapper, 'Font size').setValue('14');
    await control(wrapper, 'Diff view').setValue('side-by-side');
    await wrapper.find('form').trigger('submit');

    expect(saved(wrapper)).toEqual({
      diff: { context: 25, wrap: true, fontSize: 14 },
      ui: { diff: 'side-by-side' },
    });
  });

  it('sends a touched field even when it is back to the value shown', async () => {
    // The reader means it: the value shown may come from the repository.
    const wrapper = mount(PreferencesDialog, { props: { config: config('system') } });

    await control(wrapper, 'Tab width').setValue('8');
    await control(wrapper, 'Tab width').setValue('4');
    await wrapper.find('form').trigger('submit');

    expect(saved(wrapper).diff).toEqual({ tabWidth: 4 });
  });

  it('offers the themes of the user, then the bundled ones', () => {
    const wrapper = mount(PreferencesDialog, { props: { config: config('system') } });

    const names = themeSelect(wrapper)
      .findAll('option')
      .map((o) => o.element.value);

    expect(names).toEqual(['system', 'light', 'dark', 'midnight', 'dracula', 'nord']);
  });

  it('sends the theme the reader picked', async () => {
    const wrapper = mount(PreferencesDialog, { props: { config: config('system') } });

    await themeSelect(wrapper).setValue('dark');
    await wrapper.find('form').trigger('submit');

    expect(saved(wrapper).ui).toEqual({ theme: 'dark' });
  });

  it('sends a picked theme even when it is the one shown', async () => {
    // The file names a theme that did not load, so the page shows system.
    // Picking system on purpose must clear the name from the file.
    const wrapper = mount(PreferencesDialog, { props: { config: config('system') } });

    await themeSelect(wrapper).setValue('system');
    await wrapper.find('form').trigger('submit');

    expect(saved(wrapper).ui).toEqual({ theme: 'system' });
  });
});
