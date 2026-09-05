export function validateTelemetryCursor(value: unknown, current: number): number | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const record = value as Record<string, unknown>;
  const next = record.next_after_event_seq;
  const events = record.events;
  if (!Number.isSafeInteger(next) || (next as number) < current || !Array.isArray(events)) return null;
  let prior = current;
  for (const event of events) {
    if (!event || typeof event !== 'object' || Array.isArray(event)) return null;
    const eventSeq = (event as Record<string, unknown>).event_seq;
    if (!Number.isSafeInteger(eventSeq) || (eventSeq as number) <= prior || (eventSeq as number) > (next as number)) {
      return null;
    }
    prior = eventSeq as number;
  }
  if (events.length === 0) return next === current ? current : null;
  return prior === next ? next as number : null;
}
