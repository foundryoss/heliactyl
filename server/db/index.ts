import { SQL, sql } from 'bun';
import { resolve, dirname } from 'node:path';
import { mkdir } from 'node:fs/promises';
import type { ServerConfig } from '../../types/index';
import { readFile } from 'node:fs/promises';
import YAML from 'yaml';

export type HeliaDBClient = typeof sql & SQL;

export interface HeliaDB {
  client: HeliaDBClient;
  adapter: 'postgres' | 'mysql' | 'sqlite';
  migrate: () => Promise<void>;
}

interface SchemaColumn {
  name: string;
  type: string;
  primaryKey?: boolean;
  unique?: boolean;
  notNull?: boolean;
  default?: string | number | boolean | null;
  references?: { table: string; column: string; onDelete?: 'CASCADE' | 'SET NULL' | 'RESTRICT' };
}

interface SchemaTable {
  name: string;
  columns: SchemaColumn[];
  indexes?: { name?: string; columns: string[]; unique?: boolean }[];
}

interface SchemaSpec {
  tables: SchemaTable[];
}

export async function createHeliaDB(cfg: ServerConfig): Promise<HeliaDB> {
  const url = cfg.database.url;
  // Ensure sqlite directory exists if used
  if (url.startsWith('sqlite://') || url.startsWith('file://')) {
    const pathPart = url.replace('sqlite://', '').replace('file://', '');
    if (pathPart !== ':memory:') {
      const abs = resolve(process.cwd(), pathPart);
      const dir = dirname(abs);
      await mkdir(dir, { recursive: true }).catch(() => {});
    }
  }

  const client = new SQL(url) as unknown as HeliaDBClient;

  const adapter: HeliaDB['adapter'] = url.startsWith('mysql')
    ? 'mysql'
    : url.startsWith('sqlite') || url.startsWith('file:') || url.startsWith('file://') || url === ':memory:'
    ? 'sqlite'
    : 'postgres';

  async function migrate() {
    const schemaPath = resolve(process.cwd(), 'server', 'schema.yml');
    let spec: SchemaSpec | null = null;
    try {
      const file = await readFile(schemaPath, 'utf8');
      spec = YAML.parse(file) as SchemaSpec;
    } catch (err) {
      console.warn('[HeliaDB] No schema.yml found. Skipping automatic migrations.');
      return;
    }

    if (!spec?.tables?.length) return;

    const runRaw = async (statement: string) => {
      const anyClient = client as unknown as Record<string, any>;
      if (typeof anyClient.unsafe === 'function') {
        return await anyClient.unsafe(statement);
      }
      if (typeof anyClient.execute === 'function') {
        return await anyClient.execute(statement);
      }
      if (typeof anyClient.query === 'function') {
        return await anyClient.query(statement);
      }
      // Last resort: try using a template literal with .simple()
      try {
        // eslint-disable-next-line @typescript-eslint/ban-ts-comment
        // @ts-ignore - dynamic
        return await (client as unknown as any)([statement]).simple();
      } catch (err) {
        throw new Error('No supported raw execution method for Bun.SQL in this environment');
      }
    };

    for (const table of spec.tables) {
      const createSQL = buildCreateTableSQL(adapter, table);
      await runRaw(createSQL);

      // Non-destructive migrations: add missing columns
      try {
        const existingColumnNames = await getExistingColumns(adapter, runRaw, table.name);
        const missing = table.columns.filter((c) => !existingColumnNames.has(c.name));
        for (const col of missing) {
          const addColSQL = buildAddColumnSQL(adapter, table.name, col);
          await runRaw(addColSQL);
        }
      } catch (err) {
        console.warn(`[HeliaDB] Column sync skipped for table ${table.name}:`, (err as Error).message);
      }

      if (table.indexes && table.indexes.length) {
        for (const idx of table.indexes) {
          const idxSQL = buildCreateIndexSQL(adapter, table.name, idx);
          await runRaw(idxSQL);
        }
      }
    }
  }

  return { client, adapter, migrate };
}

function mapType(adapter: HeliaDB['adapter'], t: string): string {
  const type = t.toLowerCase();
  const isPg = adapter === 'postgres';
  const isMy = adapter === 'mysql';
  const isSq = adapter === 'sqlite';
  if (['uuid', 'guid'].includes(type)) {
    return isPg ? 'uuid' : isMy ? 'char(36)' : 'text';
  }
  if (['string', 'varchar', 'text', 'longtext'].includes(type)) {
    return type === 'string' ? (isMy ? 'varchar(255)' : isPg ? 'varchar(255)' : 'text') : type;
  }
  if (['int', 'integer'].includes(type)) return isSq ? 'integer' : 'integer';
  if (['bigint'].includes(type)) return isMy ? 'bigint' : isPg ? 'bigint' : 'integer';
  if (['boolean', 'bool'].includes(type)) return isMy ? 'boolean' : isPg ? 'boolean' : 'integer';
  if (['json', 'jsonb'].includes(type)) return isPg ? 'jsonb' : isMy ? 'json' : 'text';
  if (['datetime', 'timestamp', 'timestamptz'].includes(type)) return isPg ? 'timestamptz' : isMy ? 'datetime' : 'text';
  return 'text';
}

function buildCreateTableSQL(adapter: HeliaDB['adapter'], table: SchemaTable): string {
  const quoted = (name: string) =>
    adapter === 'mysql' ? `\`${name}\`` : adapter === 'postgres' ? `"${name}"` : `"${name}"`;

  const columnSQL: string[] = [];
  const pkColumns: string[] = [];

  for (const col of table.columns) {
    const parts: string[] = [quoted(col.name), mapType(adapter, col.type)];
    if (col.notNull) parts.push('NOT NULL');
    if (col.unique) parts.push('UNIQUE');
    if (col.default !== undefined && col.default !== null) {
      if (typeof col.default === 'string' && col.default.toLowerCase() === 'now') {
        parts.push('DEFAULT ' + (adapter === 'mysql' ? 'CURRENT_TIMESTAMP' : 'CURRENT_TIMESTAMP'));
      } else {
        const val = typeof col.default === 'string' ? `'${col.default}'` : String(col.default);
        parts.push('DEFAULT ' + val);
      }
    }
    if (col.references) {
      const ref = col.references;
      const onDelete = ref.onDelete ? ` ON DELETE ${ref.onDelete}` : '';
      parts.push(
        `REFERENCES ${quoted(ref.table)}(${quoted(ref.column)})${onDelete}`
      );
    }
    columnSQL.push(parts.join(' '));
    if (col.primaryKey) pkColumns.push(col.name);
  }

  if (pkColumns.length) {
    columnSQL.push(`PRIMARY KEY (${pkColumns.map((c) => quoted(c)).join(', ')})`);
  }

  const create = `CREATE TABLE IF NOT EXISTS ${quoted(table.name)} (\n  ${columnSQL.join(',\n  ')}\n);`;
  return create;
}

async function getExistingColumns(
  adapter: HeliaDB['adapter'],
  runRaw: (sql: string) => Promise<any>,
  tableName: string
): Promise<Set<string>> {
  const cols = new Set<string>();
  if (adapter === 'sqlite') {
    const rows = (await runRaw(`PRAGMA table_info("${tableName}")`)) as Array<{ name: string }>;
    for (const r of rows || []) if (r && r.name) cols.add(r.name);
    return cols;
  }
  if (adapter === 'postgres') {
    const rows = (await runRaw(
      `SELECT column_name as name FROM information_schema.columns WHERE table_schema = 'public' AND table_name = '${tableName}'`
    )) as Array<{ name: string }>;
    for (const r of rows || []) if (r && (r as any).name) cols.add((r as any).name);
    return cols;
  }
  // mysql
  const rows = (await runRaw(
    `SELECT COLUMN_NAME as name FROM information_schema.columns WHERE table_schema = DATABASE() AND table_name = '${tableName}'`
  )) as Array<{ name: string }>;
  for (const r of rows || []) if (r && (r as any).name) cols.add((r as any).name);
  return cols;
}

function buildAddColumnSQL(
  adapter: HeliaDB['adapter'],
  tableName: string,
  col: SchemaColumn
): string {
  const quoted = (name: string) =>
    adapter === 'mysql' ? `\`${name}\`` : adapter === 'postgres' ? `"${name}"` : `"${name}"`;
  const parts: string[] = [quoted(col.name), mapType(adapter, col.type)];
  if (col.notNull) parts.push('NOT NULL');
  if (col.unique) parts.push('UNIQUE');
  if (col.default !== undefined && col.default !== null) {
    if (typeof col.default === 'string' && col.default.toLowerCase() === 'now') {
      parts.push('DEFAULT ' + (adapter === 'mysql' ? 'CURRENT_TIMESTAMP' : 'CURRENT_TIMESTAMP'));
    } else {
      const val = typeof col.default === 'string' ? `'${col.default}'` : String(col.default);
      parts.push('DEFAULT ' + val);
    }
  }
  if (col.references) {
    const ref = col.references;
    const onDelete = ref.onDelete ? ` ON DELETE ${ref.onDelete}` : '';
    parts.push(`REFERENCES ${quoted(ref.table)}(${quoted(ref.column)})${onDelete}`);
  }
  return `ALTER TABLE ${quoted(tableName)} ADD COLUMN ${parts.join(' ')};`;
}

function buildCreateIndexSQL(
  adapter: HeliaDB['adapter'],
  table: string,
  idx: { name?: string; columns: string[]; unique?: boolean }
): string {
  const quoted = (name: string) =>
    adapter === 'mysql' ? `\`${name}\`` : adapter === 'postgres' ? `"${name}"` : `"${name}"`;
  const indexName = idx.name || `${table}_${idx.columns.join('_')}_idx`;
  const unique = idx.unique ? 'UNIQUE ' : '';
  return `CREATE ${unique}INDEX IF NOT EXISTS ${quoted(indexName)} ON ${quoted(table)} (${idx.columns
    .map((c) => quoted(c))
    .join(', ')});`;
}


