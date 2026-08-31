const dayFormatter = new Intl.DateTimeFormat('zh-CN', {
  month: 'short',
  day: 'numeric',
  weekday: 'short',
});

const timestampFormatter = new Intl.DateTimeFormat('zh-CN', {
  month: 'short',
  day: 'numeric',
  hour: '2-digit',
  minute: '2-digit',
  hour12: false,
});

export function formatDueDate(value: string): string {
  const date = new Date(`${value}T12:00:00`);
  return Number.isNaN(date.getTime()) ? value : dayFormatter.format(date);
}

export function formatCaptureTime(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : timestampFormatter.format(date);
}

export function shortId(value: string | null): string {
  return value ? value.slice(0, 6) : 'no-id';
}

export function revisionCode(value: string): string {
  return value.length <= 8 ? value : value.slice(-8);
}
