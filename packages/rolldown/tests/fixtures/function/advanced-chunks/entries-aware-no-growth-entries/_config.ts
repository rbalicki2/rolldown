import { defineTest } from 'rolldown-tests';
import { expect } from 'vitest';

export default defineTest({
  config: {
    input: ['main.js', 'admin.js'],
    output: {
      codeSplitting: {
        groups: [
          {
            name: 'lib',
            test: /lib-/,
            entriesAware: true,
            entriesAwareMaxAddedBytesPerSavedRequest: 100,
            entriesAwareLoadRoots: [['main.js', 'page-a.js']],
            entriesAwareCountLoadRootsOnly: true,
            entriesAwareFreeDynamicEntryLoads: true,
            entriesAwareNoGrowthEntries: ['main.js'],
          },
        ],
      },
    },
  },
  afterTest(output) {
    // Merging `lib-shell.js` with `lib-a.js` would save the load root a request, but `main.js`
    // would then load `lib-a.js` at startup.
    const libChunks = output.output
      .filter((chunk) => chunk.type === 'chunk' && chunk.fileName.startsWith('lib'))
      .map((chunk) =>
        chunk.type === 'chunk' ? chunk.moduleIds.map((id) => id.split(/[\\/]/).pop()) : [],
      );
    expect(libChunks).toHaveLength(2);
    expect(libChunks).toContainEqual(['lib-shell.js']);
    expect(libChunks).toContainEqual(['lib-a.js']);
  },
});
