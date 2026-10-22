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
            entriesAwareMaxAddedBytesPerSavedRequest: 50,
            entriesAwareIsolate: [['page-adm*.js']],
          },
        ],
      },
    },
  },
  afterTest(output) {
    // Merging `lib-a.js` with `lib-admin.js` would save both admin pages a request, but
    // `page-a.js` would then download `lib-admin.js`, which only the isolated pages reach.
    const libChunks = output.output
      .filter((chunk) => chunk.type === 'chunk' && chunk.fileName.startsWith('lib'))
      .map((chunk) =>
        chunk.type === 'chunk' ? chunk.moduleIds.map((id) => id.split(/[\\/]/).pop()) : [],
      );
    expect(libChunks).toHaveLength(2);
    expect(libChunks).toContainEqual(['lib-a.js']);
    expect(libChunks).toContainEqual(['lib-admin.js']);
  },
});
