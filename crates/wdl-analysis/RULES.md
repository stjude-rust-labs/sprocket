# Rules

This table documents all implemented `wdl` analysis rules implemented on the `main`
branch of the `stjude-rust-labs/wdl` repository. Note that the information may
be out of sync with released packages.

## Analysis Rules

| Name                    | Description                                                                                               |
|:------------------------|:----------------------------------------------------------------------------------------------------------|
| UnnecessaryFunctionCall | Ensures that function calls are necessary.                                                                |
| UnusedCall              | Ensures that outputs of a call statement are used in the declaring workflow.                              |
| UnusedDeclaration       | Ensures that private declarations in tasks or workspaces are used within the declaring task or workspace. |
| UnusedImport            | Ensures that import namespaces are used in the importing document.                                        |
| UnusedInput             | Ensures that task or workspace inputs are used within the declaring task or workspace.                    |
