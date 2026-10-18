/** The `gh pr create` line the Git view shows. Display only; the host builds the real argv. */
export function prCommand(input: { title: string; base: string; head: string; draft: boolean }): string {
  const title = input.title.replace(/\r?\n/g, ' ')
  const draft = input.draft ? ' --draft' : ''
  return `gh pr create --title ${JSON.stringify(title)} --body-file - --base ${JSON.stringify(input.base)} --head ${JSON.stringify(input.head)}${draft}`
}

/** Why the Open pull request button is disabled, or nothing when it can be used. */
export function prBlockedReason(feature: boolean, gh: 'found' | 'missing' | undefined): string | undefined {
  if (!feature) return 'Needs a newer acpd on the host'
  if (gh !== 'found') return 'GitHub CLI (gh) was not found on the host'
  return undefined
}

/** `origin/main` is how git names the remote branch; `gh --base` wants `main`. */
export function prBase(name: string): string {
  return name.replace(/^origin\//, '')
}

/** A pull request needs a branch other than the one it merges into. The base is usually the remote
 * branch (`origin/main`) and the current branch the local one (`main`), so compare the plain names. */
export function isBaseBranch(base: string, current: string): boolean {
  return Boolean(base) && prBase(base) === current
}
