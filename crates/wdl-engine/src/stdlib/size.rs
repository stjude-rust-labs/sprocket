//! Implements the `size` function from the WDL standard library.

use std::borrow::Cow;
use std::fs;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use wdl_analysis::types::PrimitiveType;
use wdl_ast::Diagnostic;

use super::CallContext;
use super::Function;
use super::Signature;
use crate::CompoundValue;
use crate::PrimitiveValue;
use crate::StorageUnit;
use crate::Value;
use crate::diagnostics::function_call_failed;
use crate::diagnostics::invalid_storage_unit;

/// Determines the size of a file, directory, or the sum total sizes of the
/// files/directories contained within a compound value. The files may be
/// optional values; None values have a size of 0.0. By default, the size is
/// returned in bytes unless the optional second argument is specified with a
/// unit.
///
/// https://github.com/openwdl/wdl/blob/wdl-1.2/SPEC.md#glob
fn size(context: CallContext<'_>) -> Result<Value, Diagnostic> {
    debug_assert!(!context.arguments.is_empty() && context.arguments.len() < 3);
    debug_assert!(context.return_type_eq(PrimitiveType::Float));

    let unit = if context.arguments.len() == 2 {
        let unit = context
            .coerce_argument(1, PrimitiveType::String)
            .unwrap_string();

        unit.parse()
            .map_err(|_| invalid_storage_unit(&unit, context.arguments[1].span))?
    } else {
        StorageUnit::default()
    };

    // If the first argument is a string, we need to check if it's a file or
    // directory and treat it as such.
    let value = match context.arguments[0].value.as_string() {
        Some(s) => {
            let path = context.work_dir().join(s.as_str());
            let metadata = path
                .metadata()
                .with_context(|| {
                    format!(
                        "failed to read metadata for file `{path}`",
                        path = path.display()
                    )
                })
                .map_err(|e| function_call_failed("size", format!("{e:?}"), context.call_site))?;
            if metadata.is_dir() {
                PrimitiveValue::Directory(s.clone()).into()
            } else {
                PrimitiveValue::File(s.clone()).into()
            }
        }
        _ => context.arguments[0].value.clone(),
    };

    calculate_disk_size(&value, unit, context.work_dir())
        .map_err(|e| function_call_failed("size", format!("{e:?}"), context.call_site))
        .map(Into::into)
}

/// Used to calculate the disk size of a value.
///
/// The value may be a file or a directory or a compound type containing files
/// or directories.
///
/// The size of a directory is based on the sum of the files contained in the
/// directory.
fn calculate_disk_size(value: &Value, unit: StorageUnit, cwd: &Path) -> Result<f64> {
    match value {
        Value::None => Ok(0.0),
        Value::Primitive(v) => primitive_disk_size(v, unit, cwd),
        Value::Compound(v) => compound_disk_size(v, unit, cwd),
        Value::Task(_) => bail!("the size of a task variable cannot be calculated"),
        Value::Hints(_) => bail!("the size of a hints value cannot be calculated"),
        Value::Input(_) => bail!("the size of an input value cannot be calculated"),
        Value::Output(_) => bail!("the size of an output value cannot be calculated"),
        Value::Call(_) => bail!("the size of a call value cannot be calculated"),
    }
}

/// Calculates the disk size of the given primitive value in the given unit.
fn primitive_disk_size(value: &PrimitiveValue, unit: StorageUnit, cwd: &Path) -> Result<f64> {
    match value {
        PrimitiveValue::File(path) => {
            let path = cwd.join(path.as_str());
            let metadata = path.metadata().with_context(|| {
                format!(
                    "failed to read metadata for file `{path}`",
                    path = path.display()
                )
            })?;

            if !metadata.is_file() {
                bail!("path `{path}` is not a file", path = path.display());
            }

            Ok(unit.units(metadata.len()))
        }
        PrimitiveValue::Directory(path) => calculate_directory_size(&cwd.join(path.as_str()), unit),
        _ => Ok(0.0),
    }
}

/// Calculates the disk size for a compound value in the given unit.
fn compound_disk_size(value: &CompoundValue, unit: StorageUnit, cwd: &Path) -> Result<f64> {
    match value {
        CompoundValue::Pair(pair) => Ok(calculate_disk_size(pair.left(), unit, cwd)?
            + calculate_disk_size(pair.right(), unit, cwd)?),
        CompoundValue::Array(array) => Ok(array.as_slice().iter().try_fold(0.0, |t, e| {
            anyhow::Ok(t + calculate_disk_size(e, unit, cwd)?)
        })?),
        CompoundValue::Map(map) => Ok(map.iter().try_fold(0.0, |t, (k, v)| {
            anyhow::Ok(
                t + match k {
                    Some(k) => primitive_disk_size(k, unit, cwd)?,
                    None => 0.0,
                } + calculate_disk_size(v, unit, cwd)?,
            )
        })?),
        CompoundValue::Object(object) => Ok(object.iter().try_fold(0.0, |t, (_, v)| {
            anyhow::Ok(t + calculate_disk_size(v, unit, cwd)?)
        })?),
        CompoundValue::Struct(s) => Ok(s.iter().try_fold(0.0, |t, (_, v)| {
            anyhow::Ok(t + calculate_disk_size(v, unit, cwd)?)
        })?),
    }
}

/// Calculates the size of the given directory in the given unit.
fn calculate_directory_size(path: &Path, unit: StorageUnit) -> Result<f64> {
    // Don't follow symlinks as a security measure
    let metadata = path.symlink_metadata().with_context(|| {
        format!(
            "failed to read metadata for directory `{path}`",
            path = path.display()
        )
    })?;

    if !metadata.is_dir() {
        bail!("path `{path}` is not a directory", path = path.display());
    }

    // Create a queue for processing directories
    let mut queue: Vec<Cow<'_, Path>> = Vec::new();
    queue.push(path.into());

    // Process each directory in the queue, adding the sizes of its files
    let mut size = 0.0;
    while let Some(path) = queue.pop() {
        for entry in fs::read_dir(&path)? {
            let entry = entry.with_context(|| {
                format!(
                    "failed to read entry of directory `{path}`",
                    path = path.display()
                )
            })?;

            // Note: `DirEntry::metadata` doesn't follow symlinks
            let metadata = entry.metadata().with_context(|| {
                format!(
                    "failed to read metadata for file `{path}`",
                    path = entry.path().display()
                )
            })?;
            if metadata.is_dir() {
                queue.push(entry.path().into());
            } else {
                size += unit.units(metadata.len());
            }
        }
    }

    Ok(size)
}

/// Gets the function describing `size`.
pub const fn descriptor() -> Function {
    Function::new(
        const {
            &[
                Signature::new("(None, <String>) -> Float", size),
                Signature::new("(File?, <String>) -> Float", size),
                Signature::new("(String?, <String>) -> Float", size),
                Signature::new("(Directory?, <String>) -> Float", size),
                Signature::new(
                    "(X, <String>) -> Float where `X`: any compound type that recursively \
                     contains a `File` or `Directory`",
                    size,
                ),
            ]
        },
    )
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use wdl_ast::version::V1;

    use crate::PrimitiveValue;
    use crate::v1::test::TestEnv;
    use crate::v1::test::eval_v1_expr;

    #[test]
    fn size() {
        let mut env = TestEnv::default();

        // 10 byte file
        env.write_file("foo", "0123456789");
        // 20 byte file
        env.write_file("bar", "01234567890123456789");
        // 30 byte file
        env.write_file("baz", "012345678901234567890123456789");

        env.insert_name(
            "file",
            PrimitiveValue::new_file(
                env.work_dir()
                    .join("bar")
                    .to_str()
                    .expect("should be UTF-8"),
            ),
        );
        env.insert_name(
            "dir",
            PrimitiveValue::new_directory(env.work_dir().to_str().expect("should be UTF-8")),
        );

        let diagnostic = eval_v1_expr(&mut env, V1::Two, "size('foo', 'invalid')").unwrap_err();
        assert_eq!(
            diagnostic.message(),
            "invalid storage unit `invalid`; supported units are `B`, `KB`, `K`, `MB`, `M`, `GB`, \
             `G`, `TB`, `T`, `KiB`, `Ki`, `MiB`, `Mi`, `GiB`, `Gi`, `TiB`, and `Ti`"
        );

        let diagnostic =
            eval_v1_expr(&mut env, V1::Two, "size('does-not-exist', 'B')").unwrap_err();
        assert!(
            diagnostic
                .message()
                .starts_with("call to function `size` failed: failed to read metadata for file")
        );

        let source = format!("size('{path}', 'B')", path = env.work_dir().display());
        let value = eval_v1_expr(&mut env, V1::Two, &source).unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 60.0);

        for (expected, unit) in [
            (10.0, "B"),
            (0.01, "K"),
            (0.01, "KB"),
            (0.00001, "M"),
            (0.00001, "MB"),
            (0.00000001, "G"),
            (0.00000001, "GB"),
            (0.00000000001, "T"),
            (0.00000000001, "TB"),
            (0.009765625, "Ki"),
            (0.009765625, "KiB"),
            (0.0000095367431640625, "Mi"),
            (0.0000095367431640625, "MiB"),
            (0.000000009313225746154785, "Gi"),
            (0.000000009313225746154785, "GiB"),
            (0.000000000009094947017729282, "Ti"),
            (0.000000000009094947017729282, "TiB"),
        ] {
            let value = eval_v1_expr(&mut env, V1::Two, &format!("size('foo', '{unit}')")).unwrap();
            approx::assert_relative_eq!(value.unwrap_float(), expected);
        }

        let value = eval_v1_expr(&mut env, V1::Two, "size(None, 'B')").unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 0.0);

        let value = eval_v1_expr(&mut env, V1::Two, "size(file, 'B')").unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 20.0);

        let value = eval_v1_expr(&mut env, V1::Two, "size(dir, 'B')").unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 60.0);

        let value = eval_v1_expr(&mut env, V1::Two, "size((dir, dir), 'B')").unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 120.0);

        let value = eval_v1_expr(&mut env, V1::Two, "size([file, file, file], 'B')").unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 60.0);

        let value = eval_v1_expr(
            &mut env,
            V1::Two,
            "size({ 'a': file, 'b': file, 'c': file }, 'B')",
        )
        .unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 60.0);

        let value = eval_v1_expr(
            &mut env,
            V1::Two,
            "size(object { a: file, b: file, c: file }, 'B')",
        )
        .unwrap();
        approx::assert_relative_eq!(value.unwrap_float(), 60.0);
    }
}
