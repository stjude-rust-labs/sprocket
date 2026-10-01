-- Removes the resource-utilization column from the `tasks` table.
--
-- This is lossy: utilization recorded by the forward migration is discarded.

alter table tasks drop column utilization;
