-- Removes the external ID column from the `tasks` table.
--
-- This is lossy: external IDs recorded by the forward migration are discarded.

alter table tasks drop column external_id;
