import { statSync } from 'node:fs';
import { isAbsolute } from 'node:path';
import { createInterface } from 'node:readline';
import { DatabaseSync } from 'node:sqlite';

const MAX_FRAME_BYTES = 16_384;
const MAX_QUERY_CHARS = 4_000;
const MAX_RECORD_CHARS = 7_000;
const MAX_TOTAL_CHARS = 28_000;

function fail(code) {
  process.stdout.write(`${JSON.stringify({ ok: false, code })}\n`);
}

function tokens(query) {
  return [...query.toLowerCase().matchAll(/[a-z0-9][a-z0-9_.-]*/gu)]
    .map((match) => match[0])
    .filter((token) => token.length >= 2)
    .slice(0, 48);
}

function validate(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('REQUEST_INVALID');
  const allowed = new Set(['schema', 'db_path', 'query', 'limit', 'deadline_ms']);
  if (Object.keys(value).some((key) => !allowed.has(key))) throw new Error('REQUEST_FIELDS_INVALID');
  if (value.schema !== 'apocrypha.anamnesis.read-request.v1') throw new Error('REQUEST_SCHEMA_INVALID');
  if (typeof value.db_path !== 'string' || !isAbsolute(value.db_path) || !statSync(value.db_path).isFile()) {
    throw new Error('DATABASE_INVALID');
  }
  if (typeof value.query !== 'string' || !value.query.trim() || value.query.length > MAX_QUERY_CHARS || value.query.includes('\0')) {
    throw new Error('QUERY_INVALID');
  }
  if (!Number.isInteger(value.limit) || value.limit < 1 || value.limit > 12) throw new Error('LIMIT_INVALID');
  if (!Number.isInteger(value.deadline_ms) || value.deadline_ms < 250 || value.deadline_ms > 30_000) {
    throw new Error('DEADLINE_INVALID');
  }
  return value;
}

function queryRows(database, query, limit) {
  const terms = tokens(query);
  if (terms.length) {
    const expression = terms.map((term) => `"${term.replaceAll('"', '""')}"`).join(' OR ');
    try {
      const rows = database.prepare(
        'SELECT r.id,r.ts,r.session,r.repo,r.kind,r.ref,r.payload,r.payload_sha,r.self_sha,r.provenance,' +
        'bm25(records_fts) AS rank FROM records_fts JOIN records r ON r.id=records_fts.rowid ' +
        'WHERE records_fts MATCH ? AND r.redacted=0 ORDER BY rank,r.id DESC LIMIT ?',
      ).all(expression, limit);
      if (rows.length) return rows;
    } catch {
      // FTS is optional. The read-only LIKE path below remains available.
    }
  }
  const fallbackTerms = terms.length ? terms : [query.toLowerCase()];
  const clause = fallbackTerms.map(() => 'lower(COALESCE(payload,\'\')) LIKE ?').join(' OR ');
  const needle = `%${query.toLowerCase()}%`;
  return database.prepare(
    `SELECT id,ts,session,repo,kind,ref,payload,payload_sha,self_sha,provenance,NULL AS rank FROM records ` +
    `WHERE redacted=0 AND (${clause} OR lower(COALESCE(ref,'')) LIKE ? OR lower(kind) LIKE ?) ` +
    'ORDER BY id DESC LIMIT ?',
  ).all(...fallbackTerms.map((term) => `%${term}%`), needle, needle, limit);
}

function recall(request) {
  const database = new DatabaseSync(request.db_path, { readOnly: true, timeout: request.deadline_ms });
  try {
    database.exec('PRAGMA query_only=ON');
    const rows = queryRows(database, request.query.trim(), request.limit);
    let remaining = MAX_TOTAL_CHARS;
    const records = [];
    for (const row of rows) {
      if (remaining <= 0) break;
      const text = String(row.payload ?? '').trim().slice(0, Math.min(MAX_RECORD_CHARS, remaining));
      if (!text) continue;
      records.push({
        id: String(row.id),
        text,
        metadata: {
          kind: String(row.kind ?? ''),
          ref: String(row.ref ?? ''),
          repo: String(row.repo ?? ''),
          session: String(row.session ?? ''),
          provenance: String(row.provenance ?? ''),
          recorded_at: String(row.ts ?? ''),
          payload_sha: String(row.payload_sha ?? ''),
          self_sha: String(row.self_sha ?? ''),
          rank: typeof row.rank === 'number' ? row.rank : null,
        },
      });
      remaining -= text.length;
    }
    return { ok: true, schema: 'apocrypha.anamnesis.read-result.v1', authority: 'none', read_only: true, records };
  } finally {
    database.close();
  }
}

let consumed = false;
const input = createInterface({ input: process.stdin, crlfDelay: Infinity });
input.on('line', (line) => {
  if (consumed) return;
  consumed = true;
  if (Buffer.byteLength(line, 'utf8') > MAX_FRAME_BYTES) return fail('REQUEST_TOO_LARGE');
  try {
    const result = recall(validate(JSON.parse(line)));
    process.stdout.write(`${JSON.stringify(result)}\n`);
  } catch (error) {
    const code = error instanceof Error && /^[A-Z][A-Z0-9_]{2,79}$/u.test(error.message)
      ? error.message : 'ANAMNESIS_READ_FAILED';
    fail(code);
  }
});
input.on('close', () => {
  if (!consumed) fail('REQUEST_REQUIRED');
});
