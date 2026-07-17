# Docker Image Locking Design

`sprocket dev lock` creates a complete snapshot of every container source in an analyzed WDL closure, and `sprocket run` automatically enforces the nearest snapshot. The feature pins mutable Docker and OCI references to manifest digests, verifies local SIF file contents, and never lets a task start with an unlocked or changed container. An image-index digest fixes the available platform manifests but still lets the runtime select the host's platform.

## Goals

The lock supports two related workflows. A workflow author can publish a release with the exact container manifests and SIF contents used at release time. A workflow runner can also preserve those container inputs so a later run uses the same content even when mutable tags have moved.

The implementation must:

- lock every task in the analyzed source and import closure;
- support legacy `runtime.container` and `runtime.docker` values as well as modern `requirements.container` scalar and array values;
- include the configured default container when a task omits a container or uses the `*` wildcard;
- resolve Docker and OCI registry tags without requiring a Docker daemon;
- reuse credentials from Docker's standard configuration and credential helpers;
- checksum local SIF files;
- apply one strict policy across Docker, TES, and Apptainer backends;
- preserve current execution behavior when no lock file is discovered; and
- accept existing unversioned experimental lock files while writing the versioned format.

## Non-Goals

The first finished version does not evaluate dynamic container expressions during lock generation, plan an entire workflow before execution, copy SIF contents into the lock, or add Sprocket-specific registry credentials. It rejects mutable sources that the OCI Distribution API cannot resolve, such as `library://` references, rather than omitting or passing them through.

## User Experience

`sprocket dev lock [SOURCE]` analyzes `SOURCE`, or the current directory when `SOURCE` is omitted, and writes `sprocket.lock` to the directory selected by `--output` or to the current directory. Regeneration replaces the file with a fresh, complete snapshot. The command leaves an existing lock untouched if analysis, extraction, credential lookup, registry access, hashing, serialization, or writing fails.

`sprocket run SOURCE` discovers `sprocket.lock` automatically. For a local source, discovery starts in the resolved source's directory and walks through its ancestors. Discovery stops after checking the directory containing `.git` when one exists; without a repository boundary, it reaches the filesystem root. For a remote source, discovery starts in the current directory and follows the same boundary rule. The nearest lock wins; a malformed nearest lock is an error rather than a reason to continue searching. Sprocket reports the selected lock path before preflight so strict enforcement is visible.

A discovered lock is strict. Sprocket preflights all static references, input-supplied container overrides that are known at run start, and SIF files, then checks each evaluated container candidate again immediately before creating task execution constraints. A workflow may compute a downstream container from upstream outputs, so earlier locked tasks can finish before that value becomes knowable. No task starts with an absent, unsupported, or mismatched lock entry.

`sprocket dev lock` cannot add a value that only a dynamic expression produces. Such a task can run under a lock only when its evaluated canonical reference matches an entry generated for another static reference; otherwise it fails at the task boundary. The diagnostic explains that the generator accepts only static container values.

The command has no opt-out flag. A user who does not want enforcement must remove or relocate the discovered lock.

## Architecture

The Sprocket CLI owns lock discovery, lock-file I/O, static WDL extraction, registry resolution, and SIF hashing. A shared lock model validates both newly generated and loaded data. A focused registry component normalizes image references, reads Docker credentials, follows registry authentication challenges, and resolves tags through the OCI Distribution API.

The CLI passes an immutable lock policy and its path into `wdl-engine`. The engine evaluates task requirements normally, including task input overrides, container arrays, wildcards, and the configured default. A shared resolver then converts the effective candidates before any backend receives `TaskExecutionConstraints`. This boundary keeps Docker, TES, and Apptainer behavior identical and avoids duplicating enforcement in individual pull implementations.

When no lock is present, the engine receives no lock policy and follows its current container resolution path unchanged.

## Lock Format

New files use deterministic, versioned TOML:

```toml
version = 1
generation_time = "2026-07-17T20:00:00Z"

[images]
"docker://docker.io/library/ubuntu:24.04" = "docker://docker.io/library/ubuntu@sha256:0123456789abcdef..."

[sif_files]
"images/tool.sif" = "sha256:0123456789abcdef..."
```

The `images` table maps a canonical mutable reference to a canonical digest reference. A canonical reference retains its `docker://` or `oras://` transport so two backend source types cannot collide. It also includes an explicit registry, repository, and tag. The parser applies Docker's distribution-reference grammar: the first path component denotes a registry only when it is `localhost` or contains `.` or `:`; otherwise the registry is `docker.io`. Docker Hub single-component repositories receive the `library/` namespace, and omitted tags become `latest`. Registry and repository components are lowercase, while tags retain their case. Ports remain part of the registry. A combined tag-and-digest reference is immutable; canonical output drops its redundant tag and keeps the digest. A mapped value must use SHA-256 and preserve the key's transport, registry, and repository. The parser rejects duplicate keys after canonicalization.

Registry references that already contain a digest are immutable and require no `images` entry. The engine validates their syntax and passes them through unchanged.

The `sif_files` table maps a normalized path to a SHA-256 content digest. Relative paths are relative to the directory containing `sprocket.lock`; absolute paths remain absolute. Generation uses the destination lock directory as this base even when a reference appears in a source or imported document elsewhere. A project that expects document-relative SIF paths must use absolute paths or place the lock at the intended base. The engine resolves a verified SIF reference to an absolute path before passing it to an Apptainer backend; SIF references remain unsupported by backends such as Docker and TES.

Both tables use lexical key order. `generation_time` is a quoted UTC RFC 3339 string. Registry credentials and source-document contents never appear in the file.

An unversioned file with the existing `generation_time` and `images` fields loads as the experimental legacy format. The loader accepts or ignores the legacy Chrono display timestamp rather than requiring RFC 3339, then canonicalizes and validates each image entry. The next successful `sprocket dev lock` invocation rewrites the file as version 1.

## Generation

The generator analyzes the full source and import closure with the command's effective fallback WDL version, feature flags, module configuration, and default task container. It visits every task rather than limiting the snapshot to one selected call graph. These effective settings are part of the lock's meaning: changing them can change extracted references and requires regeneration.

For each task, the generator extracts `runtime.container`, `runtime.docker`, or `requirements.container`. A container value must be a static string literal or a static array of string literals. String interpolation and all other expressions produce an error that names the task and document. The generator includes every array candidate. It replaces `*` and a missing declaration with the configured default container.

The extractor classifies each candidate:

- A mutable Docker or OCI registry reference becomes a canonical registry-resolution request.
- A digest-pinned registry reference needs no lookup or lock entry.
- A `file://` SIF reference becomes a content-hash request.
- An unsupported mutable scheme produces an error.

After deduplication, the registry resolver processes requests with bounded concurrency. It reads the registry's credentials from Docker's standard `config.json` and credential-helper chain. It uses anonymous access only when the Docker configuration has no credential for that registry; a configured helper failure is an error. The resolver records the top-level manifest or image-index SHA-256 digest so Docker retains its normal platform selection under one immutable reference.

The SIF hasher streams each file into SHA-256 rather than loading the complete image into memory. It requires every path to exist and be readable.

The generator serializes sorted TOML only after all requests succeed. It writes a temporary file in the destination directory, flushes it, and atomically replaces `sprocket.lock`. An analyzed closure with no tasks produces a valid empty lock.

## Consumption and Enforcement

`sprocket run` discovers, reads, and validates the lock after resolving the source but before creating run state. Preflight checks confirm that every static mutable image reference, effective default, and input-supplied container override known at run start has an `images` entry. They also resolve and hash every statically referenced SIF file. Extra lock entries are valid because a run may use only part of the closure. A missing effective default reports configuration drift as a likely cause and directs the user to regenerate with the current configuration.

The engine resolver applies the same normalization to every evaluated candidate. A mutable Docker or OCI candidate is replaced by the mapped digest reference. A digest-pinned reference passes through. A statically known SIF candidate uses the checksum-verified absolute path produced during preflight. A dynamic SIF candidate first encountered during evaluation resolves relative to the lock directory and is hashed at that task boundary. Unsupported schemes fail while a lock is active.

The resolver never falls back to an original mutable reference. An absent image entry, absent SIF entry, unreadable SIF, digest mismatch, or unsupported scheme fails before the affected task starts. The error identifies the task, original reference or path, lock path, and the `sprocket dev lock` remediation command.

After run-directory setup, Sprocket copies the already parsed in-memory lock snapshot into the run directory. This copy records the policy used by the run and cannot race with later edits to the project lock.

## Validation and Security

The parser validates the format version, SHA-256 syntax, canonical image names, same-repository image mappings, duplicate normalized keys, and normalized SIF paths. It does not contact a registry while reading a lock; the backend pulls the recorded digest through its existing mechanism.

Registry authentication follows Docker's registry scoping. Sprocket does not print, serialize, or retain credentials beyond the resolution request. A lock can select executable container content and therefore has the same trust implications as the WDL source and `sprocket.toml`.

## Error Handling

Generation errors are transactional: the command reports all available task/document context and preserves the prior lock. Registry and credential errors identify the canonical image and registry without exposing secrets. File errors identify both the lock-relative entry and resolved filesystem path.

Consumption errors occur before run-state creation when discovery, parsing, static preflight, or initial SIF verification fails. A dynamic runtime mismatch uses the normal failed-run path because preceding workflow work may already exist.

## Testing

Unit tests cover canonical image parsing, Docker Hub expansion, duplicate aliases, digest validation, deterministic TOML, legacy loading, nearest-lock discovery, repository boundaries, and atomic replacement. WDL extraction tests cover supported WDL versions, both runtime aliases, requirements scalars and arrays, imports, missing declarations, wildcards, configured defaults, interpolation, and unsupported schemes.

Registry tests use a local mock OCI registry and temporary Docker configuration. They cover anonymous access, inline credentials, credential helpers, bearer challenges, manifest lists, missing tags, authorization failures, helper failures, and bounded concurrent deduplication. Tests never require a Docker daemon or public registry.

SIF tests cover relative and absolute paths, streaming hashes, missing files, changed contents, duplicate normalized paths, and lock-directory-relative resolution.

Engine tests prove that each backend receives only policy-approved sources that it supports: digest-pinned Docker or OCI candidates, or checksum-verified absolute SIF paths for Apptainer backends. They cover arrays, defaults, wildcards, input overrides known at run start, dynamic downstream values, missing entries, unsupported schemes, extra entries, and no-lock compatibility.

CLI fixtures cover empty locks, fresh replacement, failure preservation, local and remote discovery roots, malformed nearest locks, provenance copying, strict run failures, and unchanged runs when no lock exists.

## Documentation

The implementation updates command help, `README.md`, and `CHANGELOG.md` with the automatic strict behavior, format examples, Docker credential reuse, SIF path rules, multi-platform manifest semantics, legacy compatibility, and regeneration workflow. It uses the repository's current documentation surfaces and does not restore the removed website documentation tree.
