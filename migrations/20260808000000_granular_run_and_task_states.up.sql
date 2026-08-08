-- no-transaction
--
-- Widens the `runs` and `tasks` status constraints to cover the more granular
-- states reported by the server.
--
-- SQLite cannot alter a CHECK constraint in place, so each table is rebuilt.
-- Dropping a table that other tables reference performs an implicit delete of
-- its rows, which trips foreign key enforcement even when constraints are
-- deferred. Foreign keys must therefore be disabled, and `pragma foreign_keys`
-- is a no-op inside a transaction, which is why this migration opts out of
-- sqlx's wrapping transaction.
--
-- Opting out of that transaction does not mean giving one up: the rebuild
-- itself runs inside an explicit transaction so that a failure part way
-- through cannot leave a database with half of its tables replaced.

pragma foreign_keys = off;

begin immediate;

-- Rebuild the runs table with `analyzing` added.
--
-- A run is `analyzing` while its WDL document and imports are being resolved
-- and type checked, which happens before the run directory is created and
-- before any evaluation begins. This leaves `queued` to mean what it says:
-- admitted, but waiting on an execution slot.
create table runs_new (
    -- Primary key
    id integer primary key not null,
    -- Public unique identifier for this run
    uuid text unique not null,
    -- Foreign key to the session that submitted this run
    session_id integer not null,
    -- Name of the run
    "name" text not null,
    -- Source WDL file path or URL
    "source" text not null,
    -- Target task or workflow name being executed (`null` when user did not
    -- provide a target and run has not yet resolved the target)
    "target" text,
    -- Current run status
    "status" text not null check("status" in ('queued', 'analyzing', 'running', 'completed', 'failed', 'canceling', 'canceled')),
    -- JSON-encoded inputs
    inputs text not null,
    -- JSON-encoded outputs
    outputs text,
    -- Error message (`null` unless the run has failed)
    "error" text,
    -- Path to the run directory (`null` when the run has not yet been started
    -- and the directory has not been created)
    directory text,
    -- Path to the indexed output directory (`null` if not indexed)
    index_directory text,
    -- Timestamp when the run started
    started_at timestamp,
    -- Timestamp when the run finished
    completed_at timestamp,
    -- Timestamp when the run was created
    created_at timestamp not null default current_timestamp,
    foreign key (session_id) references sessions(id)
);

insert into runs_new select
    id, uuid, session_id, "name", "source", "target", "status", inputs, outputs,
    "error", directory, index_directory, started_at, completed_at, created_at
from runs;

drop table runs;
alter table runs_new rename to runs;

create index idx_runs_session_id on runs(session_id);
create index idx_runs_status on runs("status");
create index idx_runs_created_at on runs(created_at);

-- Rebuild the tasks table with `initializing`, `localizing`, and `cached`
-- added.
--
-- A task row is now created as soon as the engine begins evaluating the task
-- rather than when it reaches the execution backend, so that the work leading
-- up to submission is attributable:
--
--   * `initializing` — evaluating the task's inputs, command, and requirements
--   * `localizing`   — transferring the task's inputs, which for a remote
--                      backend means digesting and uploading them
--   * `cached`       — execution was skipped entirely because a previous
--                      result was reused from the call cache
create table tasks_new (
    -- Task name from WDL
    "name" text primary key not null,
    -- Foreign key to the run managing this task
    run_id integer not null,
    -- Current task status
    "status" text not null check("status" in ('initializing', 'localizing', 'pending', 'running', 'completed', 'failed', 'canceled', 'preempted', 'cached')),
    -- Exit status from task completion
    exit_status integer,
    -- Error message (`null` unless task failed)
    "error" text,
    -- Timestamp when task was created
    created_at timestamp not null default current_timestamp,
    -- Timestamp when task started executing
    started_at timestamp,
    -- Timestamp when task reached a completed state
    completed_at timestamp,

    foreign key (run_id) references runs(id)
);

insert into tasks_new select
    "name", run_id, "status", exit_status, "error", created_at, started_at,
    completed_at
from tasks;

drop table tasks;
alter table tasks_new rename to tasks;

create index idx_tasks_run_id on tasks(run_id);
create index idx_tasks_status on tasks("status");
create index idx_tasks_created_at on tasks(created_at);

-- Fail the migration if the rebuild left any dangling reference behind.
--
-- `pragma foreign_key_check` only reports; the check constraint is what turns
-- a report into an aborted transaction.
create temp table foreign_key_violations_must_be_zero (
    count integer not null check(count = 0)
);
insert into foreign_key_violations_must_be_zero select count(*) from pragma_foreign_key_check;
drop table foreign_key_violations_must_be_zero;

commit;

pragma foreign_keys = on;
