type JsonPrimitive = boolean | null | number | string;

export type CanonicalJsonValue =
  | JsonPrimitive
  | readonly CanonicalJsonValue[]
  | { readonly [key: string]: CanonicalJsonValue };

function canonicalize(value: CanonicalJsonValue): string {
  if (
    value === null ||
    typeof value === "boolean" ||
    typeof value === "string"
  ) {
    return JSON.stringify(value);
  }
  if (typeof value === "number") {
    if (!Number.isFinite(value)) {
      throw new TypeError("Canonical JSON cannot contain non-finite numbers");
    }
    return JSON.stringify(Object.is(value, -0) ? 0 : value);
  }
  if (Array.isArray(value)) {
    return `[${value.map((entry) => canonicalize(entry)).join(",")}]`;
  }
  const record = value as {
    readonly [key: string]: CanonicalJsonValue;
  };
  const entries = Object.keys(record)
    .sort()
    .map((key) => `${JSON.stringify(key)}:${canonicalize(record[key]!)}`);
  return `{${entries.join(",")}}`;
}

function assertJsonValue(
  value: unknown,
  path = "$",
): asserts value is CanonicalJsonValue {
  if (
    value === null ||
    typeof value === "boolean" ||
    typeof value === "string"
  ) {
    return;
  }
  if (typeof value === "number") {
    if (!Number.isFinite(value)) {
      throw new TypeError(`${path} contains a non-finite number`);
    }
    return;
  }
  if (Array.isArray(value)) {
    value.forEach((entry, index) => {
      assertJsonValue(entry, `${path}[${index}]`);
    });
    return;
  }
  if (typeof value === "object") {
    for (const [key, entry] of Object.entries(value)) {
      if (entry === undefined) {
        throw new TypeError(`${path}.${key} is undefined`);
      }
      assertJsonValue(entry, `${path}.${key}`);
    }
    return;
  }
  throw new TypeError(`${path} is not a JSON value`);
}

export function canonicalJson(value: unknown): string {
  assertJsonValue(value);
  return canonicalize(value);
}
