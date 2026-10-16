/** Move `from` to the position of `target` (dropping onto a row inserts before it when moving up, after it when moving down). */
export function moveTo(ids: string[], from: string, target: string): string[] {
  const i = ids.indexOf(from), j = ids.indexOf(target)
  if (i < 0 || j < 0 || i === j) return ids
  const out = ids.filter((x) => x !== from)
  out.splice(j, 0, from)
  return out
}
