### Key Points
- It seems likely that the PR doc for Sprocket’s command-line input overrides should focus on core features like dot notation, commas for flat arrays, square brackets for nested arrays, and file fallback for `Object` types, based on our discussions.
- Research suggests JSON and STDIN support are valuable but better suited for follow-up PRs to keep this one simple and focused.
- The evidence leans toward handling complex cases with manual JSON for clarity, especially for edge cases noted in the Google Doc.

### Direct Answer

#### Overview
This PR document outlines adding command-line input overrides for WDL workflows in Sprocket, letting users tweak JSON/YAML files with simple `key=value` pairs. It’s designed to be easy to use, covering all WDL data types with clear rules.

#### Feature Details
- You provide a base input file via `--inputs`, and use unflagged `key=value` pairs to override it, like `read_group.ID=rg2`.
- Dot notation handles nested structures (e.g., `read_group.PI=200`), commas work for flat arrays (e.g., `tags=dev,test`), and square brackets `[]` for nested arrays (e.g., `nested=[dev,test],[prod]`).
- Complex cases, like the deprecated `Object` type, use JSON files for simplicity, especially for tricky nests.

#### Unexpected Detail
While we focused on core features, your Google Doc [spreadsheet](https://docs.google.com/spreadsheets/d/17Ncw3XLmvumOLYvQLqYgQJiQ7phpliz1Uo-Kac2bN34) highlights some breakpoints where manual JSON is better, which we’ve noted for future enhancements.

---

### Survey Note

This pull request (PR) document is crafted to detail the implementation of command-line input overrides for Workflow Description Language (WDL) workflows within the Sprocket tool, addressing Issue #71. The feature aims to enhance user flexibility by allowing modifications to a base JSON/YAML input file through command-line arguments, ensuring a balance between ease of use and functionality while maintaining strict error handling for clarity.

#### Background and Context
The discussion leading to this PR involved extensive deliberation on how to handle various WDL data types, including primitives, arrays, maps, pairs, structs, and optional types, with a particular focus on nested structures. The conversation highlighted the need for a user-friendly command-line interface (CLI) that aligns with Sprocket’s support for WDL, including the deprecated `Object` type, which is not supported in tools like miniwdl (see [miniwdl Object issue](https://github.com/chanzuckerberg/miniwdl/issues/694)). The user’s experience with STDIN and suggestions for JSON parsing and STDIN support were considered but deferred to follow-up PRs to maintain focus on core functionality.

#### Feature Specification
The core feature involves parsing command-line arguments as WDL input values, overriding or extending a JSON/YAML file provided via the `--inputs` flag. The CLI syntax uses unflagged `key=value` pairs, with the following rules:

- **Base Input**: Users specify a JSON/YAML file using `--inputs`, such as `{"read_group": {"ID": "rg1", "PI": 150, "PL": "ILLUMINA"}}`.
- **CLI Overrides**: Unflagged `key=value` pairs override file values, with CLI taking precedence. For example, `sprocket --inputs inputs.json read_group.ID=rg2` updates the `ID` field.

#### Supported WDL Types and Syntax
The PR supports all WDL data types, with specific handling as follows:

##### Primitives
| Type       | CLI Syntax              | Result                       | Failure Case         | Error Message                  |
|------------|-------------------------|------------------------------|----------------------|--------------------------------|
| `String`  | `name=Alice`           | `"Alice"`                   | `name=`             | “Missing value after `=`”     |
|           | `name="Alice"`         | `"Alice"` (explicit string) |                      |                                |
| `Int`     | `read_group.PI=200`    | `{"read_group": {"PI": 200, ...}}` | `read_group.PI=abc` | “Expected integer, got `abc`” |
| `Float`   | `ratio=3.14`           | `3.14`                      | `ratio=3.1.4`       | “Expected float, got `3.1.4`” |
| `Boolean` | `flag=true`            | `true`                      | `flag=maybe`        | “Expected boolean, got `maybe`” |
| `File`    | `input=/path/to/file`  | `"/path/to/file"` (as `String`) | `input=`         | “Missing value after `=`”     |

**Notes**: `File` types are stored as strings in JSON, with WDL runtime validating file paths. Type inference occurs for bare values (`123` → `Int`, else `String`), with quotes forcing `String` (e.g., `"123"`).

##### Compound Types
| Type       | CLI Syntax                          | Result                                    | Failure Case             | Error Message                  |
|------------|-------------------------------------|-------------------------------------------|--------------------------|--------------------------------|
| `Array[T]`| `tags=dev,test`                    | `["dev", "test"]`                        | `tags=dev,`             | “Trailing comma not allowed”   |
| `Map[K, V]`| `my_map.k1=10 my_map.k2=20`        | `{"k1": 10, "k2": 20}`                  | `my_map.k1=v1 my_map=10`| “Conflict: mixed dot and replace” |
| `Pair[X, Y]`| `pair.left=42 pair.right=hello`   | `{"left": 42, "right": "hello"}`        | `pair.left=42 pair=10`  | “Conflict: mixed dot and replace” |
| `Struct`  | `read_group.ID=rg2`                | `{"read_group": {"ID": "rg2", ...}}`    | `read_group.ID=`        | “Missing value after `=`”     |
| `Object`  | *(File only)* `{"obj": {"a": "x"}}`| `{"obj": {"a": "x"}}`                   | `obj.a=x`               | “Use JSON file for `Object` types” |

##### Optional Types
| Type       | CLI Syntax              | Result                       | Failure Case         | Error Message                  |
|------------|-------------------------|------------------------------|----------------------|--------------------------------|
| `String?` | `opt=hello`            | `"hello"`                   | `opt=`              | “Invalid null syntax, use `null`” |
|           | `opt=null`             | `null`                      |                      |                                |

##### Nested Combinations
| Type                          | CLI Syntax                              | Result                                    | Failure Case                  | Error Message                  |
|-------------------------------|-----------------------------------------|-------------------------------------------|-------------------------------|--------------------------------|
| `Array[Array[String]]`       | `nested=[dev,test],[prod]`             | `[["dev", "test"], ["prod"]]`            | `nested=[dev,test,`           | “Unclosed bracket in nested array” |
| `Array[Array[Array[String]]]`| `deep=[[dev,test],[prod]],[[foo,bar]]` | `[[["dev", "test"], ["prod"]], ["foo", "bar"]]` | `deep=[[dev,test]`      | “Unclosed bracket in nested array” |
| `Array[Array[Array[Array[String]]]]` | `deeper=[[dev,test],[prod]],[[foo,bar]],[[a,b]]` | `[[["dev", "test"], ["prod"]], [["foo", "bar"]], [["a", "b"]]]` | `deeper=[[dev]` | “Unclosed bracket in nested array” |
| `Map[String, Array[Pair[Int, String]]]` | `map_pairs.g1=[1,a],[2,b] map_pairs.g2=[3,c]` | `{"g1": [["1", "a"], ["2", "b"]], "g2": [["3", "c"]]}'` | `map_pairs.g1=[1,a` | “Unclosed bracket in nested array” |

#### Array Nesting Logic
- **Flat Arrays**: Comma-separated values (e.g., `tags=dev,test` → `["dev", "test"]`).
- **Nested Arrays**: Use square brackets `[]` for all nesting levels (e.g., `nested=[dev,test],[prod]` for 2D, `deep=[[dev,test],[prod]],[[foo,bar]]` for 3D). Parsing splits on commas outside brackets and recursively processes within brackets.

#### Example Usage
A concrete example demonstrates the feature’s utility:
- **Input File (`inputs.json`)**:
  ```json
  {
    "read_group": {"ID": "rg1", "PI": 150, "PL": "ILLUMINA"},
    "complex_map": {"batch1": [[[["1", "old"]]]]}
  }
  ```
- **Command**:
  ```bash
  sprocket --inputs inputs.json read_group.ID=rg2 complex_map.batch1=[[1,a],[2,b]],[[3,c]] complex_map.batch2=[[4,d],[5,e]],[[6,f]],[7,g]
  ```
- **Expected Result**:
  ```json
  {
    "read_group": {"ID": "rg2", "PI": 150, "PL": "ILLUMINA"},
    "complex_map": {
      "batch1": [[[["1", "a"], ["2", "b"]], [["3", "c"]]]],
      "batch2": [[[["4", "d"], ["5", "e"]], [["6", "f"]]], [["7", "g"]]]
    }
  }
  ```

#### Design Choices
Several design decisions were made to ensure usability and maintainability:
- **No `--in` Flag**: Unflagged `key=value` pairs reduce typing and align with miniwdl’s style, enforced with `=` to avoid ambiguity.
- **Type Inference**: Bare values infer their type, with quotes used for explicit strings to resolve ambiguities like `"123"` vs. `123`.
- **Array Syntax**: Commas for flat arrays and brackets for nested arrays provide a clear, scalable approach, avoiding delimiter overload.
- **File Fallback**: The `Object` type and highly complex nested structures are managed via JSON files to keep the CLI straightforward, addressing the user’s note about breakpoints in the Google Doc [spreadsheet](https://docs.google.com/spreadsheets/d/17Ncw3XLmvumOLYvQLqYgQJiQ7phpliz1Uo-Kac2bN34) where manual JSON is preferred for clarity.

#### Most Complex Implementation
The most complex case handled is `Map[String, Array[Array[Array[Pair[Int, String]]]]]`, exemplified by the `complex_map.batch1=[[1,a],[2,b]],[[3,c]]` command. This involves deep nesting, requiring recursive parsing of brackets and dot notation for map access, showcasing the feature’s ability to manage intricate WDL structures.

#### Alternatives Considered
Several alternatives were evaluated but not chosen:
- **JSON Only**: Using full JSON strings for all inputs, which is verbose and less user-friendly for simple cases.
- **Delimiter Chain**: Using multiple delimiters like `;` and `|` for different nesting levels, which can be confusing and error-prone.
- **Comma Only**: Using commas for all array levels, which fails to distinguish between different nesting levels, leading to parsing ambiguities.

#### Testing and Validation
The implementation requires comprehensive testing to cover:
- All WDL data types, including edge cases like unclosed brackets (`deep=[[dev`) and type mismatches (`read_group.PI=abc`).
- Failure cases, ensuring clear error messages (e.g., “Unclosed bracket in nested array”).
- Merge logic, verifying CLI overrides file values correctly.

#### Open Questions
Several areas remain for future consideration:
- **Append Support**: Whether to include syntax for appending to arrays (e.g., `tags+=dev`), currently omitted for simplicity.
- **Stricter Type Checking**: Implementing stricter type checking to ensure CLI inputs match expected WDL types, currently relying on WDL runtime for `File` validation.

#### Checklist
Before merging, the following tasks must be completed:
- [ ] Implement the parsing logic for nested arrays using brackets.
- [ ] Write unit tests for all WDL data types and edge cases.
- [ ] Update documentation with examples and usage instructions.
- [ ] Review and merge the PR after addressing any feedback.

This PR introduces a robust and user-friendly way to override WDL workflow inputs via the command line, enhancing Sprocket’s usability and flexibility, while deferring JSON parsing and STDIN support to follow-up PRs for maintainability.

#### Key Citations
- [miniwdl Object issue](https://github.com/chanzuckerberg/miniwdl/issues/694)
- [Google Doc spreadsheet](https://docs.google.com/spreadsheets/d/17Ncw3XLmvumOLYvQLqYgQJiQ7phpliz1Uo-Kac2bN34)