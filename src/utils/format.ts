export const date = (seconds: number | null): string => seconds ? new Date(seconds * 1000).toLocaleDateString() : 'Never';
export const short = (value: string | null): string => value ? value.slice(0, 12) : '—';
export const formatBytes = (bytes: number | null): string => bytes === null ? 'Unmeasured' : bytes >= 1024 ** 3 ? `${(bytes / 1024 ** 3).toFixed(1)} GB` : bytes >= 1024 ** 2 ? `${(bytes / 1024 ** 2).toFixed(0)} MB` : `${bytes} B`;
