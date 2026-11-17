-- Records the execution backend a run used in `runs.backend` as a JSON object
-- with the following keys:
--
--   * `name` — the configured name the backend was selected by (the engine's
--     `backend` setting, e.g. `default` or a key under `[backends]`).
--   * `type` — the backend type (e.g. `docker` or `slurm_apptainer`).
--   * `info` — an allowlist of identifying, non-secret backend configuration
--     fields, which varies by backend type.
--
-- `null` for runs recorded before backends were tracked or that never began
-- execution. Existing values held only the backend type and are cleared.

update runs set backend = null;
