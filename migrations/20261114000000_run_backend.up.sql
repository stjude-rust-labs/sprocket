-- Records the execution backend a run used on the `runs` table.
--
--   * `backend_type` — the backend type (e.g. `docker` or `slurm_apptainer`),
--     renamed from `backend` to distinguish it from the backend name.
--   * `backend_name` — the configured name the backend was selected by (the
--     engine's `backend` setting, e.g. `default` or a key under `[backends]`).
--   * `backend_config` — a JSON snapshot of an allowlist of identifying,
--     non-secret backend configuration fields.
--
-- All are `null` for runs recorded before they were tracked or that never
-- began execution.

alter table runs rename column backend to backend_type;
alter table runs add column backend_name text;
alter table runs add column backend_config text;
