import type { BindingChunkingContext } from '../binding.cjs';
import type { ChunkingModuleInfo } from '../options/output-options';
import type { PluginContextData } from '../plugin/plugin-context-data';
import { transformModuleInfo } from '../utils/transform-module-info';

export class ChunkingContextImpl {
  private moduleInfoCache: Map<string, ChunkingModuleInfo> | undefined = new Map();

  constructor(
    private context: BindingChunkingContext,
    private pluginContextData: PluginContextData,
  ) {}

  clearModuleInfoCache(): void {
    this.moduleInfoCache = undefined;
  }

  getModuleInfo(moduleId: string): ChunkingModuleInfo | null {
    const cached = this.moduleInfoCache?.get(moduleId);
    if (cached) {
      return cached;
    }
    const bindingInfo = this.context.getModuleInfo(moduleId);
    if (bindingInfo) {
      const option = this.pluginContextData.getModuleOption(moduleId);
      // `Object.assign` keeps the lazy `code` and `ast` getters, which a spread would call.
      const info: ChunkingModuleInfo = Object.assign(transformModuleInfo(bindingInfo, option), {
        includedLength: this.context.getModuleIncludedLength(moduleId) ?? 0,
      });
      Object.defineProperty(info, 'moduleSideEffects', {
        get: () => option.moduleSideEffects,
        set: (moduleSideEffects: ChunkingModuleInfo['moduleSideEffects']) => {
          option.moduleSideEffects = moduleSideEffects;
          option.invalidate = true;
        },
      });
      this.moduleInfoCache?.set(moduleId, info);
      return info;
    }
    return null;
  }
}
