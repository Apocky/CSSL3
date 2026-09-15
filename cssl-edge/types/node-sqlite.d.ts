// Ambient types for node:sqlite.
//
// The runtime is Node 25, where node:sqlite is built in and stable enough for the read-mostly use
// here. @types/node in this project is v20, which predates the module entirely, so TypeScript
// cannot see it. Upgrading @types/node to reach these types would change every other Node
// signature in a Next.js app at the same time -- a large, unrelated blast radius for one import.
//
// This declares only the surface actually used (open, exec, prepare, run/get/all, close). Anything
// beyond it stays a type error, which is the point: a shim that declared `any` would silence the
// next real mistake too.

declare module 'node:sqlite' {
  export interface DatabaseSyncOptions {
    readonly readOnly?: boolean;
    readonly open?: boolean;
    readonly enableForeignKeyConstraints?: boolean;
    readonly timeout?: number;
  }

  export interface StatementResultingChanges {
    readonly changes: number | bigint;
    readonly lastInsertRowid: number | bigint;
  }

  export class StatementSync {
    run(...parameters: unknown[]): StatementResultingChanges;
    get(...parameters: unknown[]): unknown;
    all(...parameters: unknown[]): unknown[];
    setReadBigInts(enabled: boolean): void;
    sourceSQL(): string;
  }

  export class DatabaseSync {
    constructor(path: string, options?: DatabaseSyncOptions);
    close(): void;
    exec(sql: string): void;
    prepare(sql: string): StatementSync;
    open(): void;
  }
}
