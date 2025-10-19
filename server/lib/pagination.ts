export interface PageParams {
  page?: number; // 1-based
  pageSize?: number; // default 20
}

export interface PageMeta {
  page: number;
  pageSize: number;
  total?: number;
  nextPage?: number | null;
  prevPage?: number | null;
}

export interface PageResponse<T> {
  items: T[];
  meta: PageMeta;
}

export function parsePageParams(input: URLSearchParams | Record<string, string | string[] | undefined>): {
  page: number;
  pageSize: number;
} {
  const get = (k: string) => {
    if (input instanceof URLSearchParams) return input.get(k) || undefined;
    const v = input[k];
    return Array.isArray(v) ? v[0] : v;
  };
  const page = Math.max(1, Number(get('page') || '1'));
  const pageSize = Math.min(100, Math.max(1, Number(get('pageSize') || '20')));
  return { page, pageSize };
}

export function buildPageMeta<T>(items: T[], page: number, pageSize: number, total?: number): PageMeta {
  const meta: PageMeta = { page, pageSize };
  if (typeof total === 'number') meta.total = total;
  meta.prevPage = page > 1 ? page - 1 : null;
  meta.nextPage = items.length === pageSize ? page + 1 : null;
  return meta;
}


