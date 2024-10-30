//! Implementation of the WDL evaluation engine.

use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use id_arena::Arena;
use id_arena::ArenaBehavior;
use id_arena::DefaultArenaBehavior;
use string_interner::DefaultStringInterner;
use wdl_analysis::document::Document;
use wdl_analysis::types::CompoundTypeDef;
use wdl_analysis::types::Type;
use wdl_analysis::types::Types;

use crate::Array;
use crate::Coercible;
use crate::CompoundValue;
use crate::CompoundValueId;
use crate::Map;
use crate::Object;
use crate::Outputs;
use crate::Pair;
use crate::Struct;
use crate::TaskInputs;
use crate::Value;
use crate::WorkflowInputs;

/// Represents a WDL evaluation engine.
#[derive(Debug, Default)]
pub struct Engine {
    /// The engine's type collection.
    types: Types,
    /// The storage arena for compound values.
    values: Arena<CompoundValue>,
    /// The string interner used to intern string/file/directory values.
    interner: DefaultStringInterner,
}

impl Engine {
    /// Constructs a new WDL evaluation engine.
    pub fn new() -> Self {
        Self::default()
    }

    /// Gets the engine's type collection.
    pub fn types(&self) -> &Types {
        &self.types
    }

    /// Gets a mutable reference to the engine's type collection.
    pub fn types_mut(&mut self) -> &mut Types {
        &mut self.types
    }

    /// Evaluates a workflow.
    ///
    /// Returns the workflow outputs upon success.
    pub async fn evaluate_workflow(
        &mut self,
        document: &Document,
        inputs: &WorkflowInputs,
    ) -> Result<Outputs> {
        let workflow = document
            .workflow()
            .ok_or_else(|| anyhow!("document does not contain a workflow"))?;
        inputs.validate(self, document, workflow).with_context(|| {
            format!(
                "failed to validate the inputs to workflow `{workflow}`",
                workflow = workflow.name()
            )
        })?;
        todo!("not yet implemented")
    }

    /// Evaluates a task with the given name.
    ///
    /// Returns the task outputs upon success.
    pub async fn evaluate_task(
        &mut self,
        document: &Document,
        name: &str,
        inputs: &TaskInputs,
    ) -> Result<Outputs> {
        let task = document
            .task_by_name(name)
            .ok_or_else(|| anyhow!("document does not contain a task named `{name}`"))?;
        inputs.validate(self, document, task).with_context(|| {
            format!(
                "failed to validate the inputs to task `{task}`",
                task = task.name()
            )
        })?;
        todo!("not yet implemented")
    }

    /// Creates a new `String` value.
    pub fn new_string(&mut self, s: impl AsRef<str>) -> Value {
        Value::String(self.interner.get_or_intern(s))
    }

    /// Creates a new `File` value.
    pub fn new_file(&mut self, s: impl AsRef<str>) -> Value {
        Value::File(self.interner.get_or_intern(s))
    }

    /// Creates a new `Directory` value.
    pub fn new_directory(&mut self, s: impl AsRef<str>) -> Value {
        Value::Directory(self.interner.get_or_intern(s))
    }

    /// Creates a new `Pair` value.
    ///
    /// Returns an error if either the `left` value or the `right` value did not
    /// coerce to the pair's `left` type or `right`` type, respectively.
    ///
    /// # Panics
    ///
    /// Panics if the given type is not a pair type from this engine's types
    /// collection or if any of the values are not from this engine.
    pub fn new_pair(
        &mut self,
        ty: Type,
        left: impl Into<Value>,
        right: impl Into<Value>,
    ) -> Result<Value> {
        if let Type::Compound(compound_ty) = ty {
            if let CompoundTypeDef::Pair(pair_ty) =
                self.types.type_definition(compound_ty.definition())
            {
                let left_ty = pair_ty.left_type();
                let right_ty = pair_ty.right_type();

                let left = left
                    .into()
                    .coerce(self, left_ty)
                    .context("failed to coerce pair's left value")?;
                left.assert_valid(self);
                let right = right
                    .into()
                    .coerce(self, right_ty)
                    .context("failed to coerce pair's right value")?;
                right.assert_valid(self);

                let id = self
                    .values
                    .alloc(CompoundValue::Pair(Pair::new(ty, left, right)));
                return Ok(Value::Compound(id));
            }
        }

        panic!(
            "type `{ty}` is not a pair type",
            ty = ty.display(&self.types)
        );
    }

    /// Creates a new `Array` value for the given array type.
    ///
    /// Returns an error if an element did not coerce to the array's element
    /// type.
    ///
    /// # Panics
    ///
    /// Panics if the given type is not an array type from this engine's types
    /// collection or if any of the values are not from this engine.
    pub fn new_array<V>(&mut self, ty: Type, elements: impl IntoIterator<Item = V>) -> Result<Value>
    where
        V: Into<Value>,
    {
        if let Type::Compound(compound_ty) = ty {
            if let CompoundTypeDef::Array(array_ty) =
                self.types.type_definition(compound_ty.definition())
            {
                let element_type = array_ty.element_type();
                let elements = elements
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let v = v.into();
                        v.assert_valid(self);
                        v.coerce(self, element_type)
                            .with_context(|| format!("failed to coerce array element at index {i}"))
                    })
                    .collect::<Result<_>>()?;
                let id = self
                    .values
                    .alloc(CompoundValue::Array(Array::new(ty, elements)));
                return Ok(Value::Compound(id));
            }
        }

        panic!(
            "type `{ty}` is not an array type",
            ty = ty.display(&self.types)
        );
    }

    /// Creates a new `Map` value.
    ///
    /// Returns an error if an key or value did not coerce to the map's key or
    /// value type, respectively.
    ///
    /// # Panics
    ///
    /// Panics if the given type is not a map type from this engine's types
    /// collection or if any of the values are not from this engine.
    pub fn new_map<K, V>(
        &mut self,
        ty: Type,
        elements: impl IntoIterator<Item = (K, V)>,
    ) -> Result<Value>
    where
        K: Into<Value>,
        V: Into<Value>,
    {
        if let Type::Compound(compound_ty) = ty {
            if let CompoundTypeDef::Map(map_ty) =
                self.types.type_definition(compound_ty.definition())
            {
                let key_type = map_ty.key_type();
                let value_type = map_ty.value_type();

                let elements = elements
                    .into_iter()
                    .enumerate()
                    .map(|(i, (k, v))| {
                        let k = k.into();
                        k.assert_valid(self);
                        let v = v.into();
                        v.assert_valid(self);
                        Ok((
                            k.coerce(self, key_type).with_context(|| {
                                format!("failed to coerce map key for element at index {i}")
                            })?,
                            v.coerce(self, value_type).with_context(|| {
                                format!("failed to coerce map value for element at index {i}")
                            })?,
                        ))
                    })
                    .collect::<Result<_>>()?;
                let id = self
                    .values
                    .alloc(CompoundValue::Map(Map::new(ty, Arc::new(elements))));
                return Ok(Value::Compound(id));
            }
        }

        panic!(
            "type `{ty}` is not a map type",
            ty = ty.display(&self.types)
        );
    }

    /// Creates a new `Object` value.
    ///
    /// # Panics
    ///
    /// Panics if any of the values are not from this engine.
    pub fn new_object<S, V>(&mut self, items: impl IntoIterator<Item = (S, V)>) -> Value
    where
        S: Into<String>,
        V: Into<Value>,
    {
        let id = self
            .values
            .alloc(CompoundValue::Object(Object::new(Arc::new(
                items
                    .into_iter()
                    .map(|(n, v)| {
                        let n = n.into();
                        let v = v.into();
                        v.assert_valid(self);
                        (n, v)
                    })
                    .collect(),
            ))));
        Value::Compound(id)
    }

    /// Creates a new struct value.
    ///
    /// Returns an error if the struct type does not contain a member of a given
    /// name or if a value does not coerce to the corresponding member's type.
    ///
    /// # Panics
    ///
    /// Panics if the given type is not a struct type from this engine's types
    /// collection or if any of the values are not from this engine.
    pub fn new_struct<S, V>(
        &mut self,
        ty: Type,
        members: impl IntoIterator<Item = (S, V)>,
    ) -> Result<Value>
    where
        S: Into<String>,
        V: Into<Value>,
    {
        if let Type::Compound(compound_ty) = ty {
            if let CompoundTypeDef::Struct(_) = self.types.type_definition(compound_ty.definition())
            {
                let members = members
                    .into_iter()
                    .map(|(n, v)| {
                        let n = n.into();
                        let v = v.into();
                        v.assert_valid(self);
                        let v = v
                            .coerce(
                                self,
                                *self
                                    .types
                                    .type_definition(compound_ty.definition())
                                    .as_struct()
                                    .expect("should be a struct")
                                    .members()
                                    .get(&n)
                                    .ok_or_else(|| {
                                        anyhow!("struct does not contain a member named `{n}`")
                                    })?,
                            )
                            .with_context(|| format!("failed to coerce struct member `{n}`"))?;
                        Ok((n, v))
                    })
                    .collect::<Result<_>>()?;
                let id = self
                    .values
                    .alloc(CompoundValue::Struct(Struct::new(ty, Arc::new(members))));
                return Ok(Value::Compound(id));
            }
        }

        panic!(
            "type `{ty}` is not a struct type",
            ty = ty.display(&self.types)
        );
    }

    /// Gets a compound value given its identifier.
    pub fn value(&self, id: CompoundValueId) -> &CompoundValue {
        &self.values[id]
    }

    /// Allocates a new compound value in the engine.
    pub(crate) fn alloc(&mut self, value: CompoundValue) -> CompoundValueId {
        self.values.alloc(value)
    }

    /// Gets the string interner of the engine.
    pub(crate) fn interner(&self) -> &DefaultStringInterner {
        &self.interner
    }

    /// Asserts that the given id comes from this engine's values arena.
    pub(crate) fn assert_same_arena(&self, id: CompoundValueId) {
        assert!(
            DefaultArenaBehavior::arena_id(id)
                == DefaultArenaBehavior::arena_id(self.values.next_id()),
            "id comes from a different values arena"
        );
    }
}
