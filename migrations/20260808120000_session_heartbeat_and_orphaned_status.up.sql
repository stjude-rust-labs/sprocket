-- no-transaction
--
-- Adds session liveness tracking and the `orphaned` terminal state.
--
-- A `sprocket server` process records a periodic heartbeat on the session it
-- owns. Runs belonging to a server session whose heartbeat has gone stale can
-- then be identified as orphaned: no live process is tracking them, so they can
-- never make progress and no cancel request against them can ever be honored.
--
-- `heartbeat_at` is nullable because sessions created before this migration
-- never recorded one. Such a session cannot belong to a live process (a live
-- process on this schema always heartbeats), so readers treat a null heartbeat
-- as "stale since `created_at`".
--
-- This migration runs outside sqlx's implicit transaction because widening a
-- check constraint requires rebuilding the table, and SQLite's documented
-- table-rebuild procedure requires `foreign_keys = off` -- a pragma that is a
-- silent no-op inside a transaction. `DROP TABLE` with foreign keys enabled
-- performs an implicit DELETE that increments the deferred-violation counter,
-- and repopulating the table via `ALTER TABLE ... RENAME` never decrements it,
-- so the commit fails. The rebuild is still wrapped in an explicit transaction
-- below, so it remains atomic.
pragma foreign_keys = off;

begin;

alter table "sessions" add column heartbeat_at timestamp;

create index idx_sessions_subcommand_heartbeat on "sessions"(subcommand, heartbeat_at);

-- Rebuild `runs` to widen the `status` check constraint with `orphaned`.
--
-- Column order and types are otherwise identical to the preceding migration.
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
    "status" text not null check("status" in ('queued', 'analyzing', 'running', 'completed', 'failed', 'canceling', 'canceled', 'orphaned')),
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

insert into runs_new (
    id, uuid, session_id, "name", "source", "target", "status", inputs, outputs,
    "error", directory, index_directory, started_at, completed_at, created_at
)
select
    id, uuid, session_id, "name", "source", "target", "status", inputs, outputs,
    "error", directory, index_directory, started_at, completed_at, created_at
from runs;

drop table runs;
alter table runs_new rename to runs;

create index idx_runs_session_id on runs(session_id);
create index idx_runs_status on runs("status");
create index idx_runs_created_at on runs(created_at);

-- Rebuild `tasks` to widen the `status` check constraint with `orphaned`.
create table tasks_new (
    -- Task name from WDL
    "name" text primary key not null,
    -- Foreign key to the run managing this task
    run_id integer not null,
    -- Current task status
    "status" text not null check("status" in ('initializing', 'localizing', 'pending', 'running', 'completed', 'failed', 'canceled', 'preempted', 'cached', 'orphaned')),
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

insert into tasks_new (
    "name", run_id, "status", exit_status, "error", created_at, started_at, completed_at
)
select
    "name", run_id, "status", exit_status, "error", created_at, started_at, completed_at
from tasks;

drop table tasks;
alter table tasks_new rename to tasks;

create index idx_tasks_run_id on tasks(run_id);
create index idx_tasks_status on tasks("status");
create index idx_tasks_created_at on tasks(created_at);

commit;

pragma foreign_keys = on;
