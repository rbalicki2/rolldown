import { defineTest } from 'rolldown-tests';
import { expect } from 'vitest';

export default defineTest({
  config: {
    output: {
      codeSplitting: {
        groups: [
          {
            name: 'lib',
            test: /lib-/,
            entriesAware: true,
            entriesAwareMaxAddedBytesPerSavedRequest: 25,
          },
        ],
      },
    },
  },
  afterTest(output) {
    // `lib-ac` and `lib-bc` merge into `lib-abc`; `lib-ab` costs too many bytes to merge.
    const libChunks = output.output
      .filter((chunk) => chunk.type === 'chunk' && chunk.fileName.startsWith('lib'))
      .map((chunk) =>
        chunk.type === 'chunk'
          ? chunk.moduleIds
              .map((id) => id.split(/[\\/]/).pop() ?? '')
              .toSorted((a, b) => a.localeCompare(b))
          : [],
      );
    expect(libChunks).toHaveLength(2);
    expect(libChunks).toContainEqual(['lib-ab.js']);
    expect(libChunks).toContainEqual(['lib-abc.js', 'lib-ac.js', 'lib-bc.js']);
  },
});
