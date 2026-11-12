-- Adds a resource-utilization column to the `tasks` table.
--
-- `utilization` is a JSON snapshot of the resource utilization observed for
-- an execution attempt (maximum/average resident memory in bytes; total,
-- user, and system CPU time in milliseconds). LSF and Slurm report usage
-- at the attempt's termination. Local Docker containers report sampled
-- usage when `backends.<name>.resource_usage_interval` is set to a nonzero
-- value. Sampling is not supported for Docker Swarm services. The column
-- remains `null` when no measurements have been reported.

alter table tasks add column utilization text;
