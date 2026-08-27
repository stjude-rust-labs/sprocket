//! Hashing utilities for analysis items.

use sha2::Digest;
use sha2::Sha256;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::v1::BoundDecl;
use wdl_ast::v1::CallStatement;
use wdl_ast::v1::CommandPart;
use wdl_ast::v1::CommandSection;
use wdl_ast::v1::ConditionalStatement;
use wdl_ast::v1::Decl;
use wdl_ast::v1::EnumDefinition;
use wdl_ast::v1::Expr;
use wdl_ast::v1::ImportSource;
use wdl_ast::v1::ImportStatement;
use wdl_ast::v1::InputSection;
use wdl_ast::v1::MetadataObjectItem;
use wdl_ast::v1::MetadataSection;
use wdl_ast::v1::MetadataValue;
use wdl_ast::v1::OutputSection;
use wdl_ast::v1::ParameterMetadataSection;
use wdl_ast::v1::PrimitiveType;
use wdl_ast::v1::RequirementsSection;
use wdl_ast::v1::RuntimeSection;
use wdl_ast::v1::ScatterStatement;
use wdl_ast::v1::StructDefinition;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::TaskHintsSection;
use wdl_ast::v1::TaskItem;
use wdl_ast::v1::Type;
use wdl_ast::v1::UnboundDecl;
use wdl_ast::v1::WorkflowDefinition;
use wdl_ast::v1::WorkflowHintsItemValue;
use wdl_ast::v1::WorkflowHintsObjectItem;
use wdl_ast::v1::WorkflowHintsSection;
use wdl_ast::v1::WorkflowItem;
use wdl_ast::v1::WorkflowStatement;

use crate::document::cache::BodyHash;
use crate::document::cache::SignatureHash;

/// Hashing for callable targets.
///
/// Unlike [`HashableItem`], this produces a hash for both the signature and the
/// body of the callable.
pub(super) trait HashableCallable {
    /// Get the signature and body hashes for this callable.
    fn hash_callable(&self) -> (SignatureHash, BodyHash);
}

impl HashableCallable for TaskDefinition {
    fn hash_callable(&self) -> (SignatureHash, BodyHash) {
        let mut signature_hasher = Sha256::default();
        let mut body_hasher = Sha256::default();

        Digest::update(&mut signature_hasher, self.keyword().text());
        Digest::update(&mut signature_hasher, self.name().text());

        for item in self.items() {
            match item {
                TaskItem::Input(i) => HashableElement::hash(&i, &mut signature_hasher),
                TaskItem::Output(o) => HashableElement::hash(&o, &mut signature_hasher),
                TaskItem::Command(c) => HashableElement::hash(&c, &mut body_hasher),
                TaskItem::Requirements(r) => HashableElement::hash(&r, &mut body_hasher),
                TaskItem::Hints(h) => HashableElement::hash(&h, &mut body_hasher),
                TaskItem::Runtime(r) => HashableElement::hash(&r, &mut body_hasher),
                TaskItem::Metadata(m) => HashableElement::hash(&m, &mut body_hasher),
                TaskItem::ParameterMetadata(p) => HashableElement::hash(&p, &mut body_hasher),
                TaskItem::Declaration(d) => HashableElement::hash(&d, &mut body_hasher),
            }
        }

        (
            signature_hasher.finalize().into(),
            body_hasher.finalize().into(),
        )
    }
}

impl HashableCallable for WorkflowDefinition {
    fn hash_callable(&self) -> (SignatureHash, BodyHash) {
        let mut signature_hasher = Sha256::default();
        let mut body_hasher = Sha256::default();

        Digest::update(&mut signature_hasher, self.keyword().text());
        Digest::update(&mut signature_hasher, self.name().text());

        for item in self.items() {
            match item {
                WorkflowItem::Input(i) => HashableElement::hash(&i, &mut signature_hasher),
                WorkflowItem::Output(o) => HashableElement::hash(&o, &mut signature_hasher),
                WorkflowItem::Conditional(c) => HashableElement::hash(&c, &mut body_hasher),
                WorkflowItem::Scatter(s) => HashableElement::hash(&s, &mut body_hasher),
                WorkflowItem::Call(c) => HashableElement::hash(&c, &mut body_hasher),
                WorkflowItem::Metadata(m) => HashableElement::hash(&m, &mut body_hasher),
                WorkflowItem::ParameterMetadata(p) => HashableElement::hash(&p, &mut body_hasher),
                WorkflowItem::Hints(h) => HashableElement::hash(&h, &mut body_hasher),
                WorkflowItem::Declaration(d) => HashableElement::hash(&d, &mut body_hasher),
            }
        }

        (
            signature_hasher.finalize().into(),
            body_hasher.finalize().into(),
        )
    }
}

/// Hashing for AST items.
pub(super) trait HashableItem {
    /// Get the signature hash for this item.
    fn hash(&self) -> SignatureHash;
}

/// Hashes text by its length and content.
fn hash_text(hasher: &mut Sha256, text: &str) {
    Digest::update(hasher, (text.len() as u64).to_le_bytes());
    Digest::update(hasher, text.as_bytes());
}

impl HashableItem for ImportStatement {
    fn hash(&self) -> SignatureHash {
        let mut signature_hasher = Sha256::default();

        match self.source() {
            ImportSource::Uri(uri) => match uri.text() {
                None => Digest::update(&mut signature_hasher, ""),
                Some(text) => hash_text(&mut signature_hasher, text.text()),
            },
            ImportSource::ModulePath(path) => {
                hash_text(&mut signature_hasher, &path.text());
            }
        }

        for alias in self.aliases() {
            let (source, target) = alias.names();
            hash_text(&mut signature_hasher, source.text());
            hash_text(&mut signature_hasher, target.text());
        }

        if let Some(members) = self.members() {
            for member in members.members() {
                hash_text(&mut signature_hasher, member.name().text());
                if let Some(alias) = member.alias() {
                    hash_text(&mut signature_hasher, alias.text());
                }
            }
        }

        if let Some(ns) = self.explicit_namespace() {
            hash_text(&mut signature_hasher, ns.text());
        }

        signature_hasher.finalize().into()
    }
}

impl HashableItem for StructDefinition {
    fn hash(&self) -> SignatureHash {
        let mut signature_hasher = Sha256::default();

        hash_text(&mut signature_hasher, self.name().text());

        for member in self.members() {
            HashableElement::hash(&member, &mut signature_hasher);
        }

        signature_hasher.finalize().into()
    }
}

impl HashableItem for EnumDefinition {
    fn hash(&self) -> SignatureHash {
        let mut signature_hasher = Sha256::default();

        hash_text(&mut signature_hasher, self.name().text());
        if let Some(ty) = self.type_parameter() {
            HashableElement::hash(&ty.ty(), &mut signature_hasher);
        }

        for choice in self.choices() {
            hash_text(&mut signature_hasher, choice.name().text());
            if let Some(value) = choice.value() {
                HashableElement::hash(&value, &mut signature_hasher);
            }
        }

        signature_hasher.finalize().into()
    }
}

/// Hashable child element of a [`HashableItem`].
trait HashableElement {
    /// Update the `hasher` with the element's contents.
    fn hash(&self, hasher: &mut Sha256);
}

impl HashableElement for Type {
    fn hash(&self, hasher: &mut Sha256) {
        if self.is_optional() {
            Digest::update(hasher, [1]);
        }
        match self {
            Type::Map(map) => {
                let (k, v) = map.types();
                HashableElement::hash(&k, hasher);
                HashableElement::hash(&v, hasher);
            }
            Type::Array(array) => {
                HashableElement::hash(&array.element_type(), hasher);
            }
            Type::Pair(pair) => {
                let (l, r) = pair.types();
                HashableElement::hash(&l, hasher);
                HashableElement::hash(&r, hasher);
            }
            Type::Object(_) => {
                hash_text(hasher, "Object");
            }
            Type::Ref(type_ref) => {
                hash_text(hasher, type_ref.name().text());
            }
            Type::Primitive(p) => {
                HashableElement::hash(p, hasher);
            }
        }
    }
}

impl HashableElement for PrimitiveType {
    fn hash(&self, hasher: &mut Sha256) {
        Digest::update(hasher, [self.kind() as u8]);
    }
}

impl HashableElement for Expr {
    fn hash(&self, hasher: &mut Sha256) {
        let range = self.inner().text_range();
        for token in std::iter::successors(self.inner().first_token(), |t| t.next_token()) {
            if !range.contains_range(token.text_range()) {
                break;
            }
            if !token.kind().is_trivia() {
                hash_text(hasher, token.text());
            }
        }
    }
}

impl HashableElement for InputSection {
    fn hash(&self, hasher: &mut Sha256) {
        for decl in self.declarations() {
            HashableElement::hash(&decl.ty(), hasher);
            hash_text(hasher, decl.name().text());
            if let Some(expr) = decl.expr() {
                HashableElement::hash(&expr, hasher);
            }
        }
    }
}

impl HashableElement for OutputSection {
    fn hash(&self, hasher: &mut Sha256) {
        for decl in self.declarations() {
            HashableElement::hash(&decl.ty(), hasher);
            hash_text(hasher, decl.name().text());
            HashableElement::hash(&decl.expr(), hasher);
        }
    }
}

impl HashableElement for UnboundDecl {
    fn hash(&self, hasher: &mut Sha256) {
        HashableElement::hash(&self.ty(), hasher);
        hash_text(hasher, self.name().text());
    }
}

impl HashableElement for BoundDecl {
    fn hash(&self, hasher: &mut Sha256) {
        HashableElement::hash(&self.ty(), hasher);
        hash_text(hasher, self.name().text());
        HashableElement::hash(&self.expr(), hasher);
    }
}

impl HashableElement for Decl {
    fn hash(&self, hasher: &mut Sha256) {
        match self {
            Decl::Bound(decl) => HashableElement::hash(decl, hasher),
            Decl::Unbound(decl) => HashableElement::hash(decl, hasher),
        }
    }
}

impl HashableElement for MetadataSection {
    fn hash(&self, hasher: &mut Sha256) {
        for i in self.items() {
            HashableElement::hash(&i, hasher);
        }
    }
}

impl HashableElement for ParameterMetadataSection {
    fn hash(&self, hasher: &mut Sha256) {
        for i in self.items() {
            HashableElement::hash(&i, hasher);
        }
    }
}

impl HashableElement for MetadataObjectItem {
    fn hash(&self, hasher: &mut Sha256) {
        hash_text(hasher, self.name().text());
        HashableElement::hash(&self.value(), hasher);
    }
}

impl HashableElement for MetadataValue {
    fn hash(&self, hasher: &mut Sha256) {
        match self {
            MetadataValue::Boolean(b) => {
                hash_text(hasher, &b.inner().text().to_string());
            }
            MetadataValue::Integer(i) => {
                hash_text(hasher, &i.inner().text().to_string());
            }
            MetadataValue::Float(f) => {
                hash_text(hasher, &f.inner().text().to_string());
            }
            MetadataValue::String(s) => {
                hash_text(hasher, &s.inner().text().to_string());
            }
            MetadataValue::Null(_) => {
                hash_text(hasher, "null");
            }
            MetadataValue::Object(o) => {
                for item in o.items() {
                    HashableElement::hash(&item, hasher);
                }
            }
            MetadataValue::Array(a) => {
                for element in a.elements() {
                    HashableElement::hash(&element, hasher);
                }
            }
        }
    }
}

impl HashableElement for ScatterStatement {
    fn hash(&self, hasher: &mut Sha256) {
        hash_text(hasher, self.variable().text());
        HashableElement::hash(&self.expr(), hasher);
        for stmt in self.statements() {
            HashableElement::hash(&stmt, hasher);
        }
    }
}

impl HashableElement for CallStatement {
    fn hash(&self, hasher: &mut Sha256) {
        for name in self.target().names() {
            hash_text(hasher, name.text());
        }
        if let Some(alias) = self.alias() {
            hash_text(hasher, alias.name().text());
        }
        for after in self.after() {
            hash_text(hasher, after.name().text());
        }
        for input in self.inputs() {
            hash_text(hasher, input.name().text());
            if let Some(expr) = input.expr() {
                HashableElement::hash(&expr, hasher);
            }
        }
    }
}

impl HashableElement for WorkflowHintsSection {
    fn hash(&self, hasher: &mut Sha256) {
        for item in self.items() {
            hash_text(hasher, item.name().text());
            HashableElement::hash(&item.value(), hasher);
        }
    }
}

impl HashableElement for WorkflowHintsObjectItem {
    fn hash(&self, hasher: &mut Sha256) {
        hash_text(hasher, self.name().text());
        HashableElement::hash(&self.value(), hasher);
    }
}

impl HashableElement for WorkflowHintsItemValue {
    fn hash(&self, hasher: &mut Sha256) {
        match self {
            WorkflowHintsItemValue::Boolean(b) => {
                hash_text(hasher, &b.inner().text().to_string());
            }
            WorkflowHintsItemValue::Integer(i) => {
                hash_text(hasher, &i.inner().text().to_string());
            }
            WorkflowHintsItemValue::Float(f) => {
                hash_text(hasher, &f.inner().text().to_string());
            }
            WorkflowHintsItemValue::String(s) => {
                hash_text(hasher, &s.inner().text().to_string());
            }
            WorkflowHintsItemValue::Object(o) => {
                for item in o.items() {
                    HashableElement::hash(&item, hasher);
                }
            }
            WorkflowHintsItemValue::Array(a) => {
                for element in a.elements() {
                    HashableElement::hash(&element, hasher);
                }
            }
        }
    }
}

impl HashableElement for CommandSection {
    fn hash(&self, hasher: &mut Sha256) {
        for part in self.parts() {
            match part {
                CommandPart::Text(t) => hash_text(hasher, t.text()),
                CommandPart::Placeholder(p) => {
                    HashableElement::hash(&p.expr(), hasher);
                }
            }
        }
    }
}

impl HashableElement for RequirementsSection {
    fn hash(&self, hasher: &mut Sha256) {
        for item in self.items() {
            hash_text(hasher, item.name().text());
            HashableElement::hash(&item.expr(), hasher);
        }
    }
}

impl HashableElement for TaskHintsSection {
    fn hash(&self, hasher: &mut Sha256) {
        for item in self.items() {
            hash_text(hasher, item.name().text());
            HashableElement::hash(&item.expr(), hasher);
        }
    }
}

impl HashableElement for RuntimeSection {
    fn hash(&self, hasher: &mut Sha256) {
        for item in self.items() {
            hash_text(hasher, item.name().text());
            HashableElement::hash(&item.expr(), hasher);
        }
    }
}

impl HashableElement for ConditionalStatement {
    fn hash(&self, hasher: &mut Sha256) {
        for clause in self.clauses() {
            if let Some(expr) = clause.expr() {
                HashableElement::hash(&expr, hasher);
            }
            for stmt in clause.statements() {
                HashableElement::hash(&stmt, hasher);
            }
        }
    }
}

impl HashableElement for WorkflowStatement {
    fn hash(&self, hasher: &mut Sha256) {
        match self {
            WorkflowStatement::Conditional(c) => HashableElement::hash(c, hasher),
            WorkflowStatement::Scatter(s) => HashableElement::hash(s, hasher),
            WorkflowStatement::Call(c) => HashableElement::hash(c, hasher),
            WorkflowStatement::Declaration(d) => HashableElement::hash(d, hasher),
        }
    }
}
