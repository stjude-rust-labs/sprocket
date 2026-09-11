-- no-transaction
--
-- Reverts session liveness tracking and the `orphaned` terminal state.
--
-- Runs and tasks recorded as `orphaned` are collapsed to `failed`, which is the
-- closest state the narrower check constraint permits. Their `error` text is
-- left intact, so the reason for the transition survives the downgrade.
--
-- See the corresponding `.up.sql` for why this runs outside sqlx's implicit
-- transaction.
pragma foreign_keys = off;

begin;

update runs set "status" = 'failed' where "status" = 'orphaned';
update tasks set "status" = 'failed' where "status" = 'orphaned';

create table runs_old (
    id integer primary key not null,
    uuid text unique not null,
    session_id integer not null,
    "name" text not null,
    "source" text not null,
    "target" text,
    "status" text not null check("status" in ('queued', 'analyzing', 'running', 'completed', 'failed', 'canceling', 'canceled')),
    inputs text not null,
    outputs text,
    "error" text,
    directory text,
    index_directory text,
    started_at timestamp,
    completed_at timestamp,
    created_at timestamp not null default current_timestamp,
    foreign key (session_id) references sessions(id)
);

insert into runs_old (
    id, uuid, session_id, "name", "source", "target", "status", inputs, outputs,
    "error", directory, index_directory, started_at, completed_at, created_at
)
select
    id, uuid, session_id, "name", "source", "target", "status", inputs, outputs,
    "error", directory, index_directory, started_at, completed_at, created_at
from runs;

drop table runs;
alter table runs_old rename to runs;

create index idx_runs_session_id on runs(session_id);
create index idx_runs_status on runs("status");
create index idx_runs_created_at on runs(created_at);

create table tasks_old (
    "name" text primary key not null,
    run_id integer not null,
    "status" text not null check("status" in ('initializing', 'localizing', 'pending', 'running', 'completed', 'failed', 'canceled', 'preempted', 'cached')),
    exit_status integer,
    "error" text,
    created_at timestamp not null default current_timestamp,
    started_at timestamp,
    completed_at timestamp,

    foreign key (run_id) references runs(id)
);

insert into tasks_old (
    "name", run_id, "status", exit_status, "error", created_at, started_at, completed_at
)
select
    "name", run_id, "status", exit_status, "error", created_at, started_at, completed_at
from tasks;

drop table tasks;
alter table tasks_old rename to tasks;

create index idx_tasks_run_id on tasks(run_id);
create index idx_tasks_status on tasks("status");
create index idx_tasks_created_at on tasks(created_at);

drop index if exists idx_sessions_subcommand_heartbeat;
alter table "sessions" drop column heartbeat_at;

commit;

pragma foreign_keys = on;
