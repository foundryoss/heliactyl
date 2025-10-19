import { readdir, stat } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import type { HeliaModule, ModuleRegisterContext, ServerConfig } from '../../types/index';

export interface ModuleLoaderResult {
  modules: HeliaModule[];
}

export async function loadModules(modulesDir = resolve(process.cwd(), 'server', 'modules')): Promise<ModuleLoaderResult> {
  const modules: HeliaModule[] = [];

  async function collect(dir: string): Promise<void> {
    const entries = await readdir(dir);
    for (const entry of entries) {
      const full = join(dir, entry);
      const st = await stat(full);
      if (st.isDirectory()) {
        const modPath = join(full, 'index.ts');
        try {
          const href = pathToFileURL(modPath).href;
          const mod = (await import(href)) as { default?: HeliaModule; module?: HeliaModule };
          const m = (mod.default || (mod as unknown as HeliaModule)) as HeliaModule;
          if (m?.manifest && typeof m.register === 'function') {
            modules.push(m);
          }
        } catch {
          // Not a module directory; recurse deeper
          await collect(full).catch(() => {});
        }
      }
    }
  }

  await collect(modulesDir);

  // order by dependencies (simple Kahn's algorithm)
  const nameToModule = new Map<string, HeliaModule>();
  const indegree = new Map<string, number>();
  const graph = new Map<string, Set<string>>();

  for (const m of modules) {
    nameToModule.set(m.manifest.name, m);
    indegree.set(m.manifest.name, 0);
    graph.set(m.manifest.name, new Set());
  }

  for (const m of modules) {
    const deps = m.manifest.dependencies || [];
    for (const d of deps) {
      if (!graph.has(d)) {
        // if dependency module is missing, still include the node
        graph.set(d, new Set());
        indegree.set(d, 0);
      }
      graph.get(d)!.add(m.manifest.name);
      indegree.set(m.manifest.name, (indegree.get(m.manifest.name) || 0) + 1);
    }
  }

  const queue: string[] = [];
  for (const [name, deg] of indegree) if (deg === 0) queue.push(name);
  const ordered: string[] = [];
  while (queue.length) {
    const n = queue.shift()!;
    ordered.push(n);
    for (const nxt of graph.get(n) || []) {
      indegree.set(nxt, (indegree.get(nxt) || 0) - 1);
      if ((indegree.get(nxt) || 0) === 0) queue.push(nxt);
    }
  }

  // Append any remaining (cycle) nodes in original order to avoid dropping them
  for (const m of modules) if (!ordered.includes(m.manifest.name)) ordered.push(m.manifest.name);

  const orderedModules = ordered
    .map((name) => nameToModule.get(name))
    .filter(Boolean) as HeliaModule[];

  return { modules: orderedModules };
}

export async function registerModules(
  ordered: HeliaModule[],
  ctx: ModuleRegisterContext
): Promise<void> {
  for (const m of ordered) {
    await m.register(ctx);
    console.log(`[ModuleLoader] Registered module: ${m.manifest.name} (${m.manifest.targetVersion})`);
  }
}


