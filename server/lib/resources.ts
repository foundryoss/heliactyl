import type { PackageResources, ServerConfig } from '../../types/index';

/** Return the package resource definition for `packageId` or throw. */
export function getPackageResources(config: ServerConfig, packageId: string): PackageResources {
  const pkg = config.packages?.items?.[packageId];
  if (!pkg) throw new Error(`Unknown package: ${packageId}`);
  return pkg;
}

/** Sum memory/disk/cpu across `servers`. */
export function sumUsedResources(servers: Array<{ memoryMb: number; diskMb: number; cpuPercent: number }>) {
  return servers.reduce(
    (acc, s) => {
      acc.memoryMb += s.memoryMb;
      acc.diskMb += s.diskMb;
      acc.cpuPercent += s.cpuPercent;
      return acc;
    },
    { memoryMb: 0, diskMb: 0, cpuPercent: 0 }
  );
}

/** Calculate remaining resources given package, extras and currently used. */
export function remainingResources(
  pkg: PackageResources,
  extra: { memoryMb: number; diskMb: number; cpuPercent: number; serverSlots: number },
  used: { memoryMb: number; diskMb: number; cpuPercent: number; servers: number }
) {
  return {
    memoryMb: Math.max(0, pkg.memoryMb + extra.memoryMb - used.memoryMb),
    diskMb: Math.max(0, pkg.diskMb + extra.diskMb - used.diskMb),
    cpuPercent: Math.max(0, pkg.cpuPercent + extra.cpuPercent - used.cpuPercent),
    serverSlots: Math.max(0, pkg.serverSlots + extra.serverSlots - used.servers),
  };
}


