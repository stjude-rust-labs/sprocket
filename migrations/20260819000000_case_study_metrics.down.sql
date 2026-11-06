-- Removes the case-study metrics columns from the `tasks` and `runs` tables.
--
-- This is lossy: submission timestamps, backend names, and transfer totals
-- recorded by the forward migration are discarded.

alter table tasks drop column submitted_at;
alter table runs drop column backend;
alter table runs drop column transfer_totals;
