# Sprocket Compatibility Policy

This document defines the compatibility guarantees for the `sprocket` command
line tool beginning with Sprocket 1.0. It applies to every Sprocket 1.x release.
Before 1.0, any interface may change without the notice required by this
policy.

## Versioning

The `sprocket` executable uses `MAJOR.MINOR.PATCH` version numbers, but it does
not use Semantic Versioning to communicate compatibility.

- Patch releases contain bug fixes and compatible behavioral corrections.
- Minor releases may add compatible features and remove features whose full
  deprecation period has elapsed.
- Major versions identify broader changes in Sprocket's functionality or
  purpose. A major version does not necessarily contain a breaking change.

This policy governs the 1.x release line. A later major release may publish a
successor policy, but changing the major version does not by itself waive the
review and notice requirements in this document.

The `wdl` and `wdl-*` Rust crates and any separately released Python bindings
follow [Semantic Versioning](https://semver.org/) according to their own package
versions. Sprocket 1.0 does not stabilize those APIs. In particular, a pre-1.0
package may make a SemVer-permitted breaking change in a minor release.

The Rust library in the root `sprocket` package is an experimental
implementation interface. It shares the executable's version and does not have
a separate SemVer compatibility guarantee. Users should not depend on the
`sprocket` crate as a library; the project neither recommends nor supports that
use.

## Stable Interfaces

Beginning with Sprocket 1.0, the following interfaces are stable unless their
documentation explicitly marks them experimental:

| Interface | Stable contract | Documentation of record |
| --- | --- | --- |
| Command line | Non-`dev` commands, options, arguments, and documented behavior | `sprocket --help`, subcommand help, and [Sprocket documentation](https://sprocket.bio/) |
| Process behavior | Documented exit statuses and placement of output on standard output or standard error | This policy, command help, and [Sprocket documentation](https://sprocket.bio/) |
| Configuration | Documented `sprocket.toml` keys, value types, defaults, and meanings | The version of `jsonschemas/sprocket.toml.json` shipped with the release for structure and types, and [configuration documentation](https://sprocket.bio/configuration/overview.html) for behavior |
| HTTP API | `/api/v1` paths, methods, parameters, request and response bodies, status codes, and documented meanings | The server's `/api/v1/openapi.json` document and [server documentation](https://sprocket.bio/subcommands/server.html) |
| Machine-readable output | Documented JSON fields, types, meanings, exit status, and output stream, including documented generated JSON files | Command help and [Sprocket documentation](https://sprocket.bio/) |
| Run artifacts | Documented `inputs.json` and `outputs.json` formats and documented `--index-on` behavior | [Provenance documentation](https://sprocket.bio/concepts/provenance.html) |

An interface that is not documented is outside this contract until its
documentation explicitly adds it. Internal Rust types and implementation
details are not made stable merely because they are public in generated API
documentation.

### Exit Status and Stream Conventions

Sprocket uses these command-wide exit statuses:

| Status | Meaning |
| --- | --- |
| `0` | The command completed successfully. |
| `1` | The command could not complete successfully, including configuration, validation, analysis, execution, or operational failures. |
| `2` | The command line could not be parsed, or an argument value failed argument-level validation, such as when a source path does not exist. |

Unless a command documents a different contract, its requested result or
machine-readable payload goes to standard output. Diagnostics, warnings,
progress, prompts, and logs go to standard error. Moving documented output to
the other stream is a breaking change.

Human-readable wording, whitespace, layout, color, progress displays,
diagnostic prose, and log messages are not stable. Scripts must not parse
human-readable output. A machine-readable format is stable only when the
documentation identifies its format and stream.

## Experimental Interfaces

Experimental interfaces may change or be removed without the stable
deprecation period. User-visible experimental changes must still appear in the
release notes.

The following interfaces are experimental or otherwise outside the executable
compatibility contract:

- Commands, options, formats, and behavior under `sprocket dev`.
- `module.json`, `module-lock.json`, module signatures, and related module
  formats while module commands remain under `sprocket dev module`.
- WDL test definitions and their schema while testing remains under
  `sprocket dev test`.
- WDL 1.4 support.
- Configuration keys and features explicitly marked experimental.
- Configuration keys that exclusively configure `sprocket dev` commands until
  those commands graduate.
- The internal run directory layout, including `runs/`, `_latest`, task attempt
  directories, and the `sprocket.db` schema. This does not include the
  documented `inputs.json` and `outputs.json` formats or documented
  `--index-on` behavior.
- The root `sprocket` Rust library API.
- APIs from separately versioned packages, such as the `wdl` and `wdl-*` Rust
  crates and any separately released Python bindings. Their package versions,
  rather than this executable policy, govern compatibility.
- Undocumented interfaces and implementation details.
- Human-readable output details excluded under
  [Exit Status and Stream Conventions](#exit-status-and-stream-conventions).

The `sprocket analyzer` command's documented invocation is stable, but LSP
capabilities and protocol behavior are stable only when Sprocket documentation
explicitly describes them.

A pull request that moves a command or feature out of `dev` must list every
command, option, file format, schema, and behavior that becomes stable. The
release notes must announce that transition.

The server must graduate from `sprocket dev server` before Sprocket 1.0 makes
`/api/v1` stable. Sprocket 1.0 must not be released while `/api/v1` is available
only through the experimental `dev` namespace.

### Database Schema

The `sprocket.db` schema is an internal implementation detail and is explicitly
not backward compatible. Sprocket may add, remove, rename, or transform tables,
columns, indexes, and stored values whenever its bundled migration can upgrade
an existing database safely. Migrations must preserve the data and behavior
needed by supported Sprocket commands, including user data those commands
manage.

Users must not query or modify the database directly. Use commands and APIs
provided by Sprocket. If Sprocket does not expose a needed operation, request a
supported command or API rather than depending on the schema.

Sprocket supports forward migration by newer releases. A database migrated by a
newer Sprocket release is not guaranteed to work with an older release.

## Compatible and Breaking Changes

Compatible changes preserve documented behavior for existing users. Examples
include:

- Adding an optional command-line option without changing existing defaults.
- Accepting an input that an earlier release rejected.
- Adding an optional `sprocket.toml` key.
- Adding an optional field to an extensible JSON object.
- Adding an HTTP endpoint without changing an existing endpoint.
- Fixing behavior that contradicts the documentation.

Consumers of extensible JSON objects must ignore fields they do not recognize.
An enumeration is not extensible unless its documentation says so; adding a
variant to a closed enumeration is a breaking change.

Breaking changes require existing users, scripts, configurations, or clients
to change. Examples include:

- Removing or renaming a command, option, argument, configuration key, API
  field, or JSON field.
- Making an optional input or field required.
- Changing a documented type, meaning, default, exit status, or output stream.
- Rejecting an input that an earlier release accepted.
- Changing an HTTP method, path, request, response, or documented status code.
- Changing behavior that matches the documentation.

When the documentation is silent but users could reasonably depend on the
behavior, maintainers must treat the change as breaking and use the deprecation
process. A breaking HTTP API design must use a new `/api/vN` prefix. Sprocket
must continue serving the previous API version through its deprecation period.

Security fixes may reject inputs or alter behavior when preserving the old
behavior would leave users vulnerable. These changes still require the
exception process below if they cannot follow the normal deprecation period.

## Deprecation

A stable interface must be deprecated before removal. Its notice must include:

- The first Sprocket release containing the deprecation and that release's
  publication date.
- The replacement and migration instructions.
- The earliest eligible removal release and date.
- An entry in the changelog and release notes.
- A notice in the relevant user documentation.
- A runtime warning when practical.

The change pull request must provide the replacement and migration
instructions. If the release number and publication date are not yet known, it
records the first deprecated release as "next release." During release
preparation, maintainers replace that marker with the release number and date
and calculate the earliest eligible removal release and date.

Removal may occur only after **both** of these conditions are true:

1. At least 90 days have passed since the release containing the first
   deprecation notice.
2. The release containing the removal is at least the second minor release
   after the release containing the first notice.

The release containing the first notice is release zero and does not count
toward the two-release threshold. Patch releases do not count. For example, an
interface first deprecated in `1.2.0` cannot be removed before `1.4.0`, and it
must remain available longer if 90 days have not passed when `1.4.0` is
published. Removal occurs in a minor or major release, never a patch release.

## Exceptions

An intentional exception to a stable 1.x guarantee requires a pull request
that:

- Identifies the affected contract.
- Explains why no compatible approach is practical.
- Describes the impact on users and automation.
- Provides migration instructions.
- Records its deprecation and release-note treatment.

Urgent security, legal, or data-integrity needs may shorten or skip the normal
notice period. They do not waive the written rationale, impact assessment,
migration guidance, or release notes.

An approved urgent exception may ship in a patch release. This is the only
exception to the rule that patch releases contain compatible fixes and do not
remove stable interfaces.

Merging an exception does not silently redefine this policy. If the exception
changes an ongoing guarantee, the same pull request must update this document.

## Policy Approval and Schema Freeze

Maintainers must approve this policy before declaring any public Sprocket 1.0
schema frozen. Policy approval and schema freeze are separate checkpoints:
approving this document defines the rules but does not itself freeze the
`sprocket.toml`, module, API, or JSON schemas.
