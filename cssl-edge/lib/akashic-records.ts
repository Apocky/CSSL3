import snapshotJson from '@/data/akashic-records.v1.json';

export interface AkashicLink {
  readonly text: string;
  readonly href: string;
  /** UTF-16 offsets into the containing paragraph's text. */
  readonly start: number;
  readonly end: number;
}

export type AkashicBlock =
  | {
      readonly kind: 'paragraph';
      readonly text: string;
      readonly links?: readonly AkashicLink[];
    }
  | {
      readonly kind: 'heading';
      readonly level: 2 | 3;
      readonly text: string;
      readonly links?: readonly AkashicLink[];
    }
  | {
      readonly kind: 'blockquote';
      readonly text: string;
      readonly links?: readonly AkashicLink[];
    }
  | { readonly kind: 'list'; readonly ordered: boolean; readonly items: readonly string[] }
  | { readonly kind: 'pre'; readonly text: string }
  | {
      readonly kind: 'figure';
      readonly alt?: string;
      readonly caption?: string;
      readonly omitted: true;
    }
  | {
      readonly kind: 'embed';
      readonly provider?: string;
      readonly href?: string;
      readonly omitted: true;
    }
  | { readonly kind: 'linkCard'; readonly text: string; readonly href: string }
  | { readonly kind: 'divider' };

export interface AkashicRecord {
  readonly slug: string;
  readonly title: string;
  readonly excerpt: string;
  readonly body: string;
  readonly blocks: readonly AkashicBlock[];
  readonly publishedAt: string;
  readonly updatedAt?: string;
  readonly year: number;
  readonly source: string;
  readonly type: string;
  readonly topics: readonly string[];
  readonly canonicalUrl?: string;
  readonly sourceUrl?: string;
  readonly sourceUrlStatus?: 'live' | 'unverified' | 'dead';
  readonly sourceSha256: string;
}

export type AkashicRecordSummary = Readonly<
  Pick<
    AkashicRecord,
    'slug' | 'title' | 'excerpt' | 'publishedAt' | 'year' | 'source' | 'type' | 'topics'
  >
>;

interface AkashicSnapshot {
  readonly schemaVersion: 1;
  readonly approvedCount: number;
  readonly draftExcludedCount: number;
  readonly sourceBytes: number;
  readonly sourceSeal: string;
  readonly snapshotDate?: string;
  readonly records: readonly AkashicRecord[];
}

const snapshot = snapshotJson as unknown as AkashicSnapshot;

export const AKASHIC_RECORD_COUNT = snapshot.approvedCount;
export const AKASHIC_DRAFT_EXCLUDED_COUNT = snapshot.draftExcludedCount;
export const AKASHIC_SOURCE_BYTES = snapshot.sourceBytes;
export const AKASHIC_SOURCE_SEAL = snapshot.sourceSeal;
export const AKASHIC_SNAPSHOT_DATE = snapshot.snapshotDate;

const records: readonly AkashicRecord[] = Object.freeze(snapshot.records);
const summaries: readonly AkashicRecordSummary[] = Object.freeze(
  records.map((record) => Object.freeze({
    slug: record.slug,
    title: record.title,
    excerpt: record.excerpt,
    publishedAt: record.publishedAt,
    year: record.year,
    source: record.source,
    type: record.type,
    topics: record.topics,
  })),
);

export function getAkashicRecords(): readonly AkashicRecord[] {
  return records;
}

export function getAkashicRecordSummaries(): readonly AkashicRecordSummary[] {
  return summaries;
}

export function findAkashicRecord(slug: string): AkashicRecord | undefined {
  return records.find((record) => record.slug === slug);
}
