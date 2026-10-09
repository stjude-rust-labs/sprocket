-- Removes the execution-metrics columns from the `tasks` table.
--
-- This is lossy: attempt attribution, resolved execution constraints, and
-- retry causes recorded by the forward migration are discarded.

alter table tasks drop column retry_cause;
alter table tasks drop column "constraints";
alter table tasks drop column attempt;
alter table tasks drop column call_id;
