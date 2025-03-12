# Input Overrides

Sprocket supports overriding input values via command-line arguments, allowing you to modify values in your input JSON/YAML file without editing it.

## Basic Usage

```bash
sprocket validate-inputs workflow.wdl --inputs inputs.json workflow.param=value
```

## Syntax

Input overrides use the format `key=value`, where:

- `key` is a dot-separated path to the input (e.g., `workflow.task.param`)
- `value` is the value to set

## Supported Types

### Primitives

```bash
# String
workflow.name=Alice
workflow.name="Alice with spaces"

# Integer
workflow.count=42

# Float
workflow.ratio=3.14

# Boolean
workflow.flag=true

# Null
workflow.optional=null
```

### Arrays

```bash
# Flat arrays (comma-separated)
workflow.tags=dev,test,prod

# Nested arrays (using brackets)
workflow.matrix=[1,2,3],[4,5,6]
workflow.deep=[[a,b],[c,d]],[[e,f]]
```

### Nested Structures

```bash
# Dot notation for nested objects
workflow.read_group.ID=rg1
workflow.read_group.PL=ILLUMINA

# Creating nested structures
workflow.new.nested.value=42
```

## Examples

### Simple Override

```bash
sprocket validate-inputs workflow.wdl --inputs inputs.json workflow.sample_name=SAMPLE_002
```

### Multiple Overrides

```bash
sprocket validate-inputs workflow.wdl --inputs inputs.json \
  workflow.sample_name=SAMPLE_002 \
  workflow.threads=8 \
  workflow.reference=/new/reference.fa
```

### Complex Structures

```bash
sprocket validate-inputs workflow.wdl --inputs inputs.json \
  workflow.read_group.ID=rg2 \
  workflow.read_group.PL=ILLUMINA \
  workflow.samples=[sample1,sample2],[sample3] \
  workflow.options.memory=16
```

## Error Handling

Common errors include:

- **Invalid syntax**: `workflow.param` (missing value)
- **Unclosed brackets**: `workflow.array=[1,2` (missing closing bracket)
- **Trailing commas**: `workflow.array=1,2,` (not allowed)
- **Path conflicts**: Setting both `workflow` and `workflow.param` (conflict)

## Limitations

- For highly complex nested structures, consider using a JSON file directly
- The `Object` type in WDL should be handled via JSON files 