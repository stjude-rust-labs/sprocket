-- Adds a resource-utilization column to the `tasks` table.
--
-- `utilization` is a JSON snapshot of the resource utilization observed for
-- an execution attempt (maximum/average resident memory in bytes; total,
-- user, and system CPU time in milliseconds). It is recorded at the
-- attempt's termination — successful, failed, or canceled alike — by
-- backends whose scheduler reports utilization (currently LSF and Slurm);
-- attempts executed by other backends leave it `null`.

alter table tasks add column utilization text;
