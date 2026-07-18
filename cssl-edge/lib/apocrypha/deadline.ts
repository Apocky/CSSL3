export class DeadlineExceededError extends Error {
  constructor(message = 'Operation exceeded its deadline.') {
    super(message);
    this.name = 'DeadlineExceededError';
  }
}

export function withDeadline<T>(
  operation: PromiseLike<T>,
  deadlineMs: number,
  onDeadline?: () => void,
): Promise<T> {
  if (!Number.isFinite(deadlineMs) || deadlineMs <= 0) {
    return Promise.reject(new RangeError('deadlineMs must be finite and greater than zero'));
  }

  return new Promise<T>((resolve, reject) => {
    let settled = false;
    const timer = setTimeout(() => {
      if (settled) return;
      settled = true;
      try {
        onDeadline?.();
      } finally {
        reject(new DeadlineExceededError());
      }
    }, deadlineMs);

    Promise.resolve(operation).then(
      (value) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        resolve(value);
      },
      (error: unknown) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        reject(error);
      },
    );
  });
}
