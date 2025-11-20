-- Metadata table
create table if not exists metadata (
    -- Metadata key identifier
    key text primary key not null,
    -- Metadata value
    value text not null,
    -- Timestamp when the metadata entry was created
    created_at timestamp not null default current_timestamp
);

insert into metadata (key, value) values ('schema_version', '1');

-- Invocations table
create table if not exists invocations (
    -- Unique identifier for this invocation
    id text primary key not null,
    -- How the runs were submitted
    method text not null check(method in ('run', 'server')),
    -- User or system that created this invocation
    created_by text not null,
    -- Timestamp when the invocation was created
    created_at timestamp not null default current_timestamp
);

-- Runs table
create table if not exists runs (
    -- Unique identifier for this run
    id text primary key not null,
    -- Foreign key to the invocation that submitted this run
    invocation_id text not null,
    -- Name of the run
    "name" text not null,
    -- Source WDL file path or URL
    source text not null,
    -- Current run status
    "status" text not null check(status in ('queued', 'running', 'completed', 'failed', 'canceling', 'canceled')),
    -- JSON-encoded inputs
    inputs text not null,
    -- JSON-encoded outputs
    outputs text,
    -- Error message if run failed
    error text,
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
    foreign key (invocation_id) references invocations(id)
);

create index idx_runs_invocation_id on runs(invocation_id);
create index idx_runs_status on runs("status");
create index idx_runs_created_at on runs(created_at);

-- Index log table
create table if not exists index_log (
    -- Unique identifier for this index log entry
    id integer primary key autoincrement not null,
    -- Foreign key to the run that created this index entry
    run_id text not null,
    -- Path to the symlink in the index directory
    index_path text not null,
    -- Path to the actual run output file being symlinked
    target_path text not null,
    -- Timestamp when the index entry was created
    created_at timestamp not null default current_timestamp,
    foreign key (run_id) references runs(id)
);

create index idx_index_log_run_id on index_log(run_id);
create index idx_index_log_index_path_created_at on index_log(index_path, created_at desc);

-- View for getting the latest index entry for each unique index path
create view latest_index_entries as
select id, run_id, index_path, target_path, created_at
from (
    select *,
           row_number() over (partition by index_path order by created_at desc) as rn
    from index_log
) ranked
where rn = 1
order by index_path;
