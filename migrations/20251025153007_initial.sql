-- Workflows table stores metadata and state for each workflow execution.
create table if not exists workflows (
    id text primary key not null,
    name text not null unique,
    status text not null check(status in ('queued', 'running', 'completed', 'failed', 'cancelled')),
    wdl_source_type text not null check(wdl_source_type in ('content', 'file')),
    wdl_source_value text not null,
    inputs json not null,
    outputs json,
    error text,
    run_directory text,
    created_at timestamp not null default current_timestamp,
    started_at timestamp,
    completed_at timestamp
);

-- Index on `status` for filtering workflows by their current state.
create index idx_workflows_status on workflows(status);

-- Index on `created_at` for chronological listing and pagination.
create index idx_workflows_created_at on workflows(created_at desc);

-- Workflow logs table stores execution logs for debugging and monitoring.
create table if not exists logs (
    id integer primary key autoincrement,
    workflow_id text not null,
    level text not null,
    message text not null,
    source text,
    created_at timestamp not null default current_timestamp,
    foreign key (workflow_id) references workflows(id) on delete cascade
);

-- Index on `workflow_id` for retrieving all logs for a specific workflow.
create index idx_logs_workflow_id on logs(workflow_id);

-- Index on `created_at` for chronological log ordering.
create index idx_logs_created_at on logs(created_at);
