// The paths a change carries.
//
// The commit message is one more file of the change, under a path that no
// git tree can hold. The reader sees a name for it, never the path.

export const COMMIT_MSG = '/COMMIT_MSG';

export function label(path: string): string {
  return path === COMMIT_MSG ? 'Commit message' : path;
}
