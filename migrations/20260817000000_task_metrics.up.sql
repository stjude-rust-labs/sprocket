-- Adds execution-metrics columns to the `tasks` table.
--
-- Each row in `tasks` is a single execution attempt (retries and
-- backend-local resubmissions each get their own row under a distinct name).
-- These columns make an attempt attributable and measurable without parsing
-- its name:
--
--   * `call_id`     — the stable WDL call path shared by every attempt of the
--                     same call (`null` for rows created before this
--                     migration or observed only through backend events).
--   * `attempt`     — the 0-based attempt number within the call.
--   * `constraints` — a JSON snapshot of the resolved execution constraints
--                     (container, cpu, memory, gpu, fpga, disks) captured
--                     when the attempt was submitted to a backend.
--   * `retry_cause` — a JSON value recording why this attempt was retried,
--                     set on the row of the attempt that failed.
--
-- Only columns are added, so no table rebuild is required.

alter table tasks add column call_id text;
alter table tasks add column attempt integer not null default 0;
alter table tasks add column "constraints" text;
alter table tasks add column retry_cause text;
