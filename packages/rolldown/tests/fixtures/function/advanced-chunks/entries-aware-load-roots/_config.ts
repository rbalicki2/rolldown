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
          },
        ],
      },
    },
  },
  afterTest(output) {
    // `main.js` and `page-a.js` load together, so their `lib` modules share one chunk. `admin.js`
    // loads `lib-shell.js` and `page-c.js` loads `lib-a.js`, so neither is a module only one entry
    // loads; `lib-b.js` is, and stays in the chunk of `page-b.js`.
    const libChunks = output.output
      .filter((chunk) => chunk.type === 'chunk' && chunk.fileName.startsWith('lib'))
      .map((chunk) =>
        chunk.type === 'chunk' ? chunk.moduleIds.map((id) => id.split(/[\\/]/).pop()) : [],
      );
    expect(libChunks).toHaveLength(1);
    expect(libChunks).toContainEqual(['lib-shell.js', 'lib-a.js']);
  },
});
