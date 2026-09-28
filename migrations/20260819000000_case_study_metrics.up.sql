-- Adds case-study metrics columns to the `tasks` and `runs` tables.
--
-- On `tasks`:
--
--   * `submitted_at` — when the attempt was submitted to an execution
--     backend (the transition to `pending`). Splits the time before
--     execution into preparation (evaluation and localization:
--     `created_at` to `submitted_at`) and scheduler queuing
--     (`submitted_at` to `started_at`).
--
-- On `runs`:
--
--   * `backend` — the name of the execution backend the run executed on,
--     recording the execution environment per run.
--   * `transfer_totals` — a JSON value totaling the bytes transferred while
--     localizing inputs and delocalizing outputs. This is a proxy for data
--     movement (e.g. egress review), not a billing figure.

alter table tasks add column submitted_at timestamp;
alter table runs add column backend text;
alter table runs add column transfer_totals text;
