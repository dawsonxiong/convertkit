const MAX_SELECTED_PAGES = 5_000;

export function isValidPageSelection(value: string): boolean {
  return getPageSelectionCount(value) !== null;
}

export function getPageSelectionCount(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) return null;

  const seen = new Set<number>();
  for (const rawPart of trimmed.split(",")) {
    const part = rawPart.trim();
    if (!part) return null;

    const pieces = part.split("-");
    if (pieces.length > 2) return null;
    const start = parsePage(pieces[0]);
    const end = pieces.length === 2 ? parsePage(pieces[1]) : start;
    if (start === null || end === null || start > end) return null;

    for (let page = start; page <= end; page += 1) {
      if (seen.has(page)) return null;
      seen.add(page);
      if (seen.size > MAX_SELECTED_PAGES) return null;
    }
  }
  return seen.size > 0 ? seen.size : null;
}

function parsePage(value: string): number | null {
  const trimmed = value.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  const page = Number(trimmed);
  return Number.isSafeInteger(page) && page > 0 ? page : null;
}
