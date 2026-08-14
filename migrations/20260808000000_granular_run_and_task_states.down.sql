-- no-transaction
--
-- Narrows the `runs` and `tasks` status constraints back to the original set
-- of states.
--
-- The rebuild drops tables that others reference, so foreign keys must be
-- disabled, and `pragma foreign_keys` is a no-op inside a transaction, which
-- is why sqlx's wrapping transaction is declined here; see the forward
-- migration for why deferring enforcement instead is not enough. The rebuild
-- still runs in the explicit transaction below.
--
-- This is lossy: rows in a state that the original schema cannot express are
-- collapsed onto the nearest state it can. An `analyzing` run becomes
-- `queued`, an `initializing` or `localizing` task becomes `pending`, and a
-- `cached` task becomes `completed`.

pragma foreign_keys = off;

begin immediate;

update runs set "status" = 'queued' where "status" = 'analyzing';
update tasks set "status" = 'pending' where "status" in ('initializing', 'localizing');
update tasks set "status" = 'completed' where "status" = 'cached';

create table runs_old (
    id integer primary key not null,
    uuid text unique not null,
    session_id integer not null,
    "name" text not null,
    "source" text not null,
    "target" text,
    "status" text not null check("status" in ('queued', 'running', 'completed', 'failed', 'canceling', 'canceled')),
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

insert into runs_old select
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
    "status" text not null check("status" in ('pending', 'running', 'completed', 'failed', 'canceled', 'preempted')),
    exit_status integer,
    "error" text,
    created_at timestamp not null default current_timestamp,
    started_at timestamp,
    completed_at timestamp,

    foreign key (run_id) references runs(id)
);

insert into tasks_old select
    "name", run_id, "status", exit_status, "error", created_at, started_at,
    completed_at
from tasks;

drop table tasks;
alter table tasks_old rename to tasks;

create index idx_tasks_run_id on tasks(run_id);
create index idx_tasks_status on tasks("status");
create index idx_tasks_created_at on tasks(created_at);

create temp table foreign_key_violations_must_be_zero (
    count integer not null check(count = 0)
);
insert into foreign_key_violations_must_be_zero select count(*) from pragma_foreign_key_check;
drop table foreign_key_violations_must_be_zero;

commit;

pragma foreign_keys = on;
