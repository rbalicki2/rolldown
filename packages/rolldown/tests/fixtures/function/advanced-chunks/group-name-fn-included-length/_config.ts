import { defineTest } from 'rolldown-tests';
import { expect } from 'vitest';

const libId = /[\\/]lib\.js$/;
let libIncludedLength: number | undefined;
let libCodeLength: number | undefined;

export default defineTest({
  config: {
    output: {
      codeSplitting: {
        groups: [
          {
            name(id, ctx) {
              if (!libId.test(id)) {
                return null;
              }
              const info = ctx.getModuleInfo(id)!;
              libIncludedLength = info.includedLength;
              libCodeLength = info.code?.length;
              return 'lib';
            },
          },
        ],
      },
    },
  },
  afterTest() {
    // Only `export const used = 'used';` survives tree shaking.
    expect(libIncludedLength).toBe("export const used = 'used';".length);
    expect(libCodeLength).toBeGreaterThan(libIncludedLength!);
  },
});
