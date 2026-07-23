# Transient Module Transaction State

## Goal

Module mutations retain serialized, crash-recoverable updates of `module.json` and `module-lock.json` without leaving a `.sprocket` directory in a healthy project after a command finishes.

## Architecture

The mutation lock moves from the module project to a user-level Sprocket lock directory. Its filename is a digest of the canonical module root, so paths that reach the same project through different aliases acquire the same advisory file lock without exposing the project path in a filename. Failure to canonicalize the module root is a hard error.

The lock directory uses this precedence:

1. `<config_root>/locks/module-mutations`
2. `<system_cache>/sprocket/locks/module-mutations`
3. `<system_temp>/sprocket/locks/module-mutations`

Selection depends only on whether each base directory is available. Once selected, failure to create or use that lock directory is a hard error rather than a reason to fall through to another directory, which could let two processes choose different locks for the same project.

The global lock file remains on disk after release. Removing an advisory lock file creates a race in which waiters can hold the unlinked file while another process creates and locks a replacement.

Recovery snapshots remain project-local because they must stay coupled to the exact files they restore. The journal uses `.sprocket/module-mutation.pending` while snapshots are being prepared and `.sprocket/module-mutation` after the complete journal becomes active.

Existing projects may contain the legacy `.sprocket/module-mutation.lock`. After acquiring the global lock, Sprocket acquires this legacy lock before recovery and holds both locks through acquire-time recovery and cleanup. It then drops the legacy lock handle and removes the recognized lock file while still holding the global lock, regardless of whether an active journal existed. Any subsequent mutation proceeds under the global lock alone. This migration waits for an older Sprocket process that already holds the legacy lock; running older and newer Sprocket binaries concurrently during the rest of the command is otherwise unsupported.

## Mutation Lifecycle

Acquiring a `LockedProject` performs these steps:

1. Canonicalize the module root and acquire its global mutation lock.
2. Acquire the legacy local lock when it exists.
3. Remove an incomplete pending journal when one exists.
4. Restore both project files when an active journal exists.
5. Remove the active journal when recovery occurred.
6. Drop the legacy lock handle and remove its file when step 2 acquired it, even when no recovery occurred.
7. Remove `.sprocket` when the preceding cleanup leaves it empty.
8. Reload `module.json` while still holding the global lock.

Lock acquisition does not create `.sprocket` when no recovery state exists.

Committing an update performs these steps:

1. Validate and serialize every proposed output before creating a journal.
2. Create `.sprocket/module-mutation.pending`.
3. Snapshot each existing project file or record that it was absent.
4. Synchronize the snapshots, atomically rename the pending journal to `module-mutation`, and synchronize the state directory so the active journal is durable.
5. Write and synchronize the requested project files.
6. Delete the active journal and synchronize its parent directory.
7. Remove `.sprocket` when it is empty.

An ordinary write failure restores both snapshots immediately, removes the active journal, and removes `.sprocket` when empty. A process crash leaves the active journal for the next command to recover. Commit, rollback, and recovery perform cleanup before releasing the global lock. If `.sprocket` contains unrelated entries, transaction cleanup preserves the directory and those entries.

## Error Handling

Failures that affect transaction correctness remain hard errors. These include creating or activating the journal, reading or restoring snapshots, synchronizing project files, and removing an active recovery journal.

Once the active journal is safely removed, failure to remove the legacy lock file emits a warning and leaves it for a later command to retry. Cleanup then attempts one non-recursive `remove_dir` call for `.sprocket`. It must never use recursive removal or check for emptiness before removal. A missing or non-empty directory needs no warning; any other failure emits a warning. The command must not report a committed update, completed rollback, or completed recovery as failed solely because cosmetic cleanup failed.

Journal entries must remain regular files and directories rather than symbolic links. Recovery must never follow a symbolic link or remove unrelated project contents.

## Testing

Tests cover:

- successful manifest and lockfile updates leave no `.sprocket`;
- lock acquisition without a mutation creates no project-local state;
- an ordinary write failure rolls back both files and removes `.sprocket`;
- an interrupted mutation is recovered by the next command and `.sprocket` is removed;
- an originally absent lockfile is removed during rollback;
- unrelated entries under `.sprocket` survive transaction cleanup;
- a legacy local mutation lock is acquired and removed during migration, including when no journal exists;
- two concurrent mutation handles for the same canonical project prove that the second blocks until the first releases the global lock;
- different path aliases, including a symbolic-link alias, resolve to the same global lock and serialize;
- malformed or symbolic-link journal entries remain rejected;
- failure to remove an empty state directory does not change a successful commit into an error.

## Non-Goals

This design does not move recovery snapshots into the global cache, remove crash recovery, or provide a general filesystem transaction API. It applies only to coordinated mutations of a module project's manifest and lockfile.
