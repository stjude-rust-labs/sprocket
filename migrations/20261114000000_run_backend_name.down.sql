-- Removes the `backend_name` column from the `runs` table.
--
-- This is lossy: the configured backend name recorded for each run is
-- discarded.

alter table runs drop column backend_name;
