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
    -- How the workflows were submitted
    method text not null check(method in ('cli', 'http')),
    -- Optional user or system that created this invocation
    created_by text,
    -- Timestamp when the invocation was created
    created_at timestamp not null default current_timestamp
);

-- Workflows table
create table if not exists workflows (
    -- Unique identifier for this workflow execution
    id text primary key not null,
    -- Foreign key to the invocation that submitted this workflow
    invocation_id text not null,
    -- Name of the workflow
    "name" text not null,
    -- Source WDL file path or URL
    source text not null,
    -- Current execution status
    "status" text not null check(status in ('pending', 'running', 'completed', 'failed', 'cancelled')),
    -- JSON-encoded workflow inputs
    inputs text not null,
    -- JSON-encoded workflow outputs
    outputs text,
    -- Error message if workflow failed
    error text,
    -- Path to the workflow execution directory
    execution_dir text not null,
    -- Timestamp when the workflow started executing
    started_at timestamp,
    -- Timestamp when the workflow finished executing
    completed_at timestamp,
    -- Timestamp when the workflow was created
    created_at timestamp not null default current_timestamp,
    foreign key (invocation_id) references invocations(id)
);

create index idx_workflows_invocation_id on workflows(invocation_id);
create index idx_workflows_status on workflows("status");
create index idx_workflows_created_at on workflows(created_at);

-- Index log table
create table if not exists index_log (
    -- Unique identifier for this index log entry
    id integer primary key autoincrement not null,
    -- Foreign key to the workflow that created this index entry
    workflow_id text not null,
    -- Path to the symlink in the index directory
    index_path text not null,
    -- Path to the actual workflow output file being symlinked
    target_path text not null,
    -- Timestamp when the index entry was created
    created_at timestamp not null default current_timestamp,
    foreign key (workflow_id) references workflows(id)
);

create index idx_index_log_workflow_id on index_log(workflow_id);
create index idx_index_log_index_path_created_at on index_log(index_path, created_at desc);

-- View for getting the latest index entry for each unique index path
create view latest_index_entries as
select id, workflow_id, index_path, target_path, created_at
from (
    select *,
           row_number() over (partition by index_path order by created_at desc) as rn
    from index_log
) ranked
where rn = 1
order by index_path;
