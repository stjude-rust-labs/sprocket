# Introduction

::: tip Note
**Sprocket** is current an alpha-phase project. To that end,
this page serves the purpose of describing what we hope Sprocket _will_ become
rather than what it actually is today. If you're using Sprocket, we encourage
you to follow the project on
[GitHub](https://github.com/stjude-rust-labs/sprocket) to stay up to date on
progress.
:::

**Sprocket** is an bioinformatics workflow execution engine built on top of the [Workflow Description Language](https://openwdl.org). The project has multiple high-level goals, including to:

* Provide a **high-performance** workflow execution engine capable of
  orchestrating massive bioinformatics workloads (the stated target is 20,000+
  concurrent jobs).
* Develop a suite of **modern development tools** that brings bioinformatics
  development on par with other modern languages (e.g.,
  [`wdl-lsp`](https://github.com/stjude-rust-labs/wdl/tree/main/wdl-lsp)).
* Maintain an **community-focused codebase** that enables a diverse set of
  contributors from academic, non-profit, and commercial organizations.
* Build on an **open, domain-tailored standard** to ensure the toolset remains
  singularly focused on unencumbered innovation within bioinformatics.
* Retain a **simple and accessible user experience** when complexity isn't warranted.

Sprocket is written in [Rust](https://www.rust-lang.org/) and enjoys all of the
benefits that come with that choice. It also takes heavy inspiration from Rust
in terms of its approach to building developer tools that are a joy to use. The
code that drives Sprocket is split across the [`wdl`] family of crates, the
[`sprocket`] command line tool, and the [Visual Studio Code extension]
([source](https://github.com/stjude-rust-labs/sprocket-vscode)).

## Project Goals

### High-Performance Workflow Execution Engine

Fundamentally, the Sprocket project was created to address the lack of
bioinformatics workflow execution engines that can reliably handle tens of
thousands to hundreds of thousands of concurrently executing workflows across a
variety of execution environments.

The execution engine for Sprocket is comprised of two major components:

* The **orchestration engine**, which handles the scheduling and monitoring of
  units of execution within a workflow, and
* **Execution runtimes**, which carry out the work associated with a unit of
  compute within a particular environment (e.g., local compute, a high-performance compute
  cluster, or the cloud).

Briefly, the orchestration engine is generally responsible for staging anything
needed to run a job within a particular environment (localizing data, hooking up
inputs and outputs, deciding which execution runtime to dispatch jobs to)
whereas execution runtimes receive these jobs and are responsible only for
carrying them out in an independent manner.

Collectively, we consider the combination of an orchestration engine with one or
more configured execution runtimes to comprise an **execution engine**. We
envision the orchestration engine being provided by Sprocket alongside two
official execution runtimes: (a) a local runtime and a (b) Kubernetes runtime.
Beyond that, we plan to make it easy for vendors to build and maintain their own
runtimes that are available within Sprocket.

Enabling the next generation of large-scale, open science by providing a robust
and performant execution engine will always remain the top-level objective for
the project.

### Modern Development Tools

A suite of development tools are included alongside the execution engine. We believe that, when you _do_ need to dust off your code
editor and write an analysis workflow, that activity should as enjoyable as it
can be. Supporting tools, such as
[linters](https://github.com/stjude-rust-labs/wdl/tree/main/wdl-lint),
formatters, [syntax
highlighting](https://github.com/stjude-rust-labs/sprocket-vscode/blob/main/syntaxes/wdl.tmGrammar.json),
and overall [editor
integration](https://github.com/stjude-rust-labs/wdl/tree/main/wdl-lsp), are
critical to making that vision a reality.

### Community-Focused Codebase

Since the beginning, Sprocket has been architected to maximize the potential for
any interested party to effectively contribute to the codebase. This means that
(a) every part of the code is designed to be read and modified by anyone [there
are no "owners" of any part of the code base—only "experts" in a particular area
of the code], (b) strict practices, such as required documentation for public
members, a common commit message style, and `CHANGELOG.md` entries, are enforced
for each contribution, and (c) a comprehensive testing suite and CI/CD
integration mean you can tinker with confidence.

### Open, Tailored Standard

Predictability, openness, and ensured longevity are virtues the project attempts
to uphold. Given that the project is ultimately limited by the upstream workflow
language in these regards, an open standard is critical to justify such a
significant amount of community effort. Though many such standards exist,
generality is, in some respects, at odds with creating a tailored experience
within bioinformatics specifically. We've selected the [Workflow Description
Language] as the underlying standard for Sprocket, as it meets all of the above
criteria while also being relatively easy to learn and reason about.

### Simple and Accessible User Experience

Sprocket aims to provide a user experience that works from workflow development,
through local testing, and into production with fairly low overhead between
these environments. Our target audience is individuals who endeavor to develop
and deploy bioinformatics workflows with excellence. Generally speaking, this
means **bioinformaticians** and **bioinformatics software engineers** working at
a moderate to large scale, but Sprocket should work reasonably well for many
other users too. In particular, we aim to keep Sprocket as simple as we possibly
can—only introducing complexity when needed to achieve the project's other
stated goals.

## Goal-Adjacent

The following are "goal adjacent", meaning that the project values these things
when they do not otherwise inhibit the primary goals.

* **Providing robust implementations natively for a handful of reference
  backends.** Outside of a few core backends that are used to drive the
  development of the project, backends are intended to be developed and
  maintained by the backend providers themselves. When that isn't possible,
  custom backends should be able to be configured by end-users with a moderate
  amount of effort.

### Non-goals

The following are non-goals of the project.

* **Supporting multiple workflow languages or standards.** Workflow
  languages are ideally a simple means to an end: to run large-scale
  bioinformatics analyses with as little development and operational friction as
  possible. We stand unconvinced that there is a strong technical argument for
  multiple workflow languages existing within bioinformatics.
* **Supporting a comprehensive list of backends.** As stated above, we aim to
  spur a thriving community of independent backend development that is
  compatible with Sprocket. More explicitly, comprehensive support for the quirks of
  each backend (not to mention the day-to-day maintenance of these backends) represents a
  _significant_ time investment that takes away from core development time.
* **Simplicity at all costs.** Though we aim to make things simple when
  possible, complexity will be introduced when deemed necessary (this is
  particularly true when it comes to internal implementation details).
* **Native Windows compatibility.** For now, Sprocket is intended to be used on
  UNIX-like machines. If you're on Windows, we recommend you [install
  WSL](https://learn.microsoft.com/en-us/windows/wsl/install) if you haven't
  already.

## Naming

The name 'Sprocket' was chosen to evoke imagery of a set of independently
developed cogs ("bioinformatics tools") that are composed together to execute a
specific analysis ("workflows").

When discussing the command line tool, `sprocket` should be used. When
discussing the project as a whole (and in all other cases), 'Sprocket' should be
used.

[`wdl`]: https://github.com/stjude-rust-labs/wdl
[`sprocket`]: https://github.com/stjude-rust-labs/sprocket
[Visual Studio Code extension]:
    https://marketplace.visualstudio.com/items?itemName=stjude-rust-labs.sprocket-vscode
[Workflow Description Language]: https://openwdl.org
