-- The metadata table.
--
-- The metadata table is a table that is expected to exist in all versions of
-- Sprocket—it's intended to be queried by _any_ version of Sprocket to
-- determine what the version of the current database/filesystem are being used.
--
-- Other version independent information may live here at your descretion.
create table if not exists metadata (
    -- Key of the metadata element
    "key" text unique primary key not null,
    -- Value of the metadata element
    "value" text not null,
    -- Timestamp when the metadata entry was created
    created_at timestamp not null default current_timestamp
);

-- The sessions table.
--
-- Sessions are invocations of the Sprocket command line tool by a particular
-- user. They are tracked for provenance purposes.
create table if not exists "sessions" (
    -- Primary key
    id integer primary key not null,
    -- Public unique identifier for this session
    uuid text unique not null,
    -- The Sprocket subcommand used to create this session
    subcommand text not null check(subcommand in ('run', 'server')),
    -- User or account that started this session
    created_by text not null,
    -- Timestamp when the session was created
    created_at timestamp not null default current_timestamp
);

-- The runs table.
--
-- A "run" represented a targeted WDL task or workflow to execute.
create table if not exists runs (
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
    -- Current run status
    "status" text not null check("status" in ('queued', 'running', 'completed', 'failed', 'canceling', 'canceled')),
    -- JSON-encoded inputs
    inputs text not null,
    -- JSON-encoded outputs
    outputs text,
    -- Error message (`null` unless the run has failed)
    "error" text,
    -- Path to the run directory
    directory text not null,
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

create index idx_runs_session_id on runs(session_id);
create index idx_runs_status on runs("status");
create index idx_runs_created_at on runs(created_at);

-- The index log table.
--
-- The index log track _all_ entries that have ever been linked into the index.
-- Its primary purpose is providing a mechanism to reconstruct what the index
-- looked like at any point in time.
create table if not exists index_log (
    -- Primary key
    id integer primary key autoincrement not null,
    -- Foreign key to the run that created this index entry
    run_id integer not null,
    -- Path to the symlink in the index directory
    link_path text not null,
    -- Path to the actual run output file being symlinked
    target_path text not null,
    -- Timestamp when the index entry was created
    created_at timestamp not null default current_timestamp,
    foreign key (run_id) references runs(id)
);

create index idx_index_log_run_id on index_log(run_id);
create index idx_index_log_link_path_created_at on index_log(link_path, created_at desc);

-- The latest index entries view.
--
-- This is a view for getting the latest index entry for each unique link path.
-- It's useful for reconstructing what the latest complete index looks like.
create view latest_index_entries as
select id, run_id, link_path, target_path, created_at
from (
    select *,
           row_number() over (partition by link_path order by created_at desc) as rn
    from index_log
) ranked
where rn = 1
order by link_path;

-- The tasks table.
--
-- The tasks table tracks the status of tasks that execute underneath runs.
create table if not exists tasks (
    -- Task name from WDL
    "name" text primary key not null,
    -- Foreign key to the run managing this task
    run_id integer not null,
    -- Current task status
    "status" text not null check("status" in ('pending', 'running', 'completed', 'failed', 'canceled', 'preempted')),
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

create index idx_tasks_run_id on tasks(run_id);
create index idx_tasks_status on tasks("status");
create index idx_tasks_created_at on tasks(created_at);

-- The task logs table.
--
-- The table keeps track of all stdout and stderr logs from tasks.
create table if not exists task_logs (
    -- The unique ID for the task log entry
    id integer primary key autoincrement not null,
    -- A foreign key to the task that created this log
    task_name text not null,
    -- The source of the log (stderr or stdout)
    "source" text not null check("source" in ('stdout', 'stderr')),
    -- Raw log content as bytes
    chunk blob not null,
    -- Timestamp when log was received
    created_at timestamp not null default current_timestamp,

    foreign key (task_name) references tasks("name")
);

create index idx_task_logs_task on task_logs(task_name);
create index idx_task_logs_created_at on task_logs(created_at);
