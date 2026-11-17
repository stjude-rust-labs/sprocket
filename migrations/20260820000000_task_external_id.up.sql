-- Adds an external ID column to the `tasks` table.
--
-- `external_id` is the ID the execution backend assigned to an execution
-- attempt: the job ID for LSF and Slurm, or the task ID for TES. It lets a
-- restarted server find work that is still running. It is `null` for other
-- backends and for attempts that were never submitted.

alter table tasks add column external_id text;
