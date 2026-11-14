-- Adds the `backend_name` column to the `runs` table.
--
-- `runs.backend` records the backend type (e.g. `docker` or `slurm_apptainer`).
-- Several configured backends can share a type, so `backend_name` records the
-- name the run's backend was selected by (the engine's `backend` setting, e.g.
-- `default` or a key under `[backends]`). It is `null` for runs recorded
-- before this migration or that never began execution.
--
-- Only a column is added, so no table rebuild is required.

alter table runs add column backend_name text;
