//! Elements (nodes or tokens) within the AST.

use rowan::NodeOrToken;

use crate::AstNode;
use crate::AstToken;
use crate::Comment;
use crate::Ident;
use crate::SyntaxKind;
use crate::SyntaxNode;
use crate::SyntaxToken;
use crate::TreeNode;
use crate::TreeToken;
use crate::Version;
use crate::VersionStatement;
use crate::Whitespace;
use crate::v1::*;

#[macropol::macropol]
macro_rules! ast_element_impl {
    (
        // The name of the impl to create (e.g., `Node`).
        $name:ident,
        // The improper name of the impl to be displayed (e.g., `node`).
        $display:ident,
        // The implementation trait name (either `TreeNode` or `TreeToken`).
        $trait_name:ty,
        // A mapping of all of the elements to map from syntax elements to ast
        // elements.
        //
        // E.g., `command_section(): CommandSectionNode => CommandSection => CommandSection`.
        [$($suffix:ident(): $syntax_kind:ty => $inner:ty => $variant:ty),*]
    ) => {
        paste::paste! {
            impl<T: $trait_name> $name<T> {
                #[doc = "Attempts to cast an element to a [`" $name "`]."]
                pub fn cast(element: T) -> Option<Self> {
                    match element.kind() {
                        $(
                            SyntaxKind::$syntax_kind => {
                                let inner = $inner::<T>::cast(element)
                                    .expect(
                                        "couldn't cast ${stringify!($display)} to \
                                        `${stringify!($inner)}`
                                    ");

                                Some($name::$variant(inner))
                            },
                        )*
                        _ => None
                    }
                }

                #[doc = "Returns whether or not a particular [`SyntaxKind`] can cast to a [`" $name "`]."]
                pub fn can_cast(kind: &SyntaxKind) -> bool {
                    match kind {
                        $(
                            SyntaxKind::$syntax_kind => true,
                        )*
                        _ => false
                    }
                }

                #[doc = "Gets the inner type from the [`" $name "`]."]
                pub fn inner(&self) -> &T {
                    match self {
                        $(
                            $name::$variant(e) => e.inner(),
                        )*
                        // NOTE: a wildcard pattern (`_`) should not be required
                        // here. If one is suggested by the compiler, that means
                        // you're probably missing a pattern in the macros
                        // below.
                    }
                }

                $(
                    /// Attempts to get a reference to the inner [`${stringify!($inner)}`].
                    ///
                    /// * If `self` is a [`${stringify!($variant)}`], then a reference to the
                    ///   inner [`${stringify!($inner)}`] wrapped in [`Some`] is returned.
                    /// * Else, [`None`] is returned.
                    pub fn [<as_ $suffix>](&self) -> Option<&$inner<T>> {
                        match self {
                            $name::$variant($suffix) => Some($suffix),
                            _ => None,
                        }
                    }

                    /// Consumes `self` and attempts to return the inner
                    /// [`${stringify!($inner)}`].
                    ///
                    /// * If `self` is a [`${stringify!($variant)}`], then the inner
                    ///   [`${stringify!($inner)}`] wrapped in [`Some`] is returned.
                    /// * Else, [`None`] is returned.
                    pub fn [<into_ $suffix>](self) -> Option<$inner<T>> {
                        match self {
                            $name::$variant($suffix) => Some($suffix),
                            _ => None,
                        }
                    }

                    /// Consumes `self` and returns the inner [`${stringify!($inner)}`].
                    ///
                    /// # Panics
                    ///
                    /// If `self` is not a [`${stringify!($variant)}`].
                    pub fn [<unwrap_ $suffix>](self) -> $inner<T> {
                        self.[<into_ $suffix>]().expect(
                            "expected `${stringify!($variant)}` but got a different variant"
                        )
                    }
                )*
            }
        }
    };
}

/// An abstract syntax tree node.
///
/// This enum has a variant for each struct implementing the [`AstNode`] trait.
#[derive(Clone, Debug)]
pub enum Node<N: TreeNode = SyntaxNode> {
    /// An access expression.
    AccessExpr(AccessExpr<N>),
    /// An addition expression.
    AdditionExpr(AdditionExpr<N>),
    /// An array type.
    ArrayType(ArrayType<N>),
    /// A V1 abstract syntax tree.
    Ast(Ast<N>),
    /// A bound declaration.
    BoundDecl(BoundDecl<N>),
    /// An after clause in a call statement.
    CallAfter(CallAfter<N>),
    /// An alias clause in a call statement.
    CallAlias(CallAlias<N>),
    /// A call expression.
    CallExpr(CallExpr<N>),
    /// A call input item.
    CallInputItem(CallInputItem<N>),
    /// A call statement.
    CallStatement(CallStatement<N>),
    /// A target within a call statement.
    CallTarget(CallTarget<N>),
    /// A command section.
    CommandSection(CommandSection<N>),
    /// A conditional statement.
    ConditionalStatement(ConditionalStatement<N>),
    /// A conditional statement clause.
    ConditionalStatementClause(ConditionalStatementClause<N>),
    /// The `default` placeholder option.
    DefaultOption(DefaultOption<N>),
    /// A division expression.
    DivisionExpr(DivisionExpr<N>),
    /// An equality expression.
    EqualityExpr(EqualityExpr<N>),
    /// An exponentiation expression.
    ExponentiationExpr(ExponentiationExpr<N>),
    /// A greater than or equal to expression.
    GreaterEqualExpr(GreaterEqualExpr<N>),
    /// A greater than expression.
    GreaterExpr(GreaterExpr<N>),
    /// An if expression.
    IfExpr(IfExpr<N>),
    /// An import alias.
    ImportAlias(ImportAlias<N>),
    /// An import statement.
    ImportStatement(ImportStatement<N>),
    /// An index expression.
    IndexExpr(IndexExpr<N>),
    /// An inequality expression.
    InequalityExpr(InequalityExpr<N>),
    /// An input section.
    InputSection(InputSection<N>),
    /// A less than or equal to expression.
    LessEqualExpr(LessEqualExpr<N>),
    /// A less than expression.
    LessExpr(LessExpr<N>),
    /// A literal array.
    LiteralArray(LiteralArray<N>),
    /// A literal boolean.
    LiteralBoolean(LiteralBoolean<N>),
    /// A literal float.
    LiteralFloat(LiteralFloat<N>),
    /// A literal hints.
    LiteralHints(LiteralHints<N>),
    /// A literal hints item.
    LiteralHintsItem(LiteralHintsItem<N>),
    /// A literal input.
    LiteralInput(LiteralInput<N>),
    /// A literal input item.
    LiteralInputItem(LiteralInputItem<N>),
    /// A literal integer.
    LiteralInteger(LiteralInteger<N>),
    /// A literal map.
    LiteralMap(LiteralMap<N>),
    /// A literal map item.
    LiteralMapItem(LiteralMapItem<N>),
    /// A literal none.
    LiteralNone(LiteralNone<N>),
    /// A literal null.
    LiteralNull(LiteralNull<N>),
    /// A literal object.
    LiteralObject(LiteralObject<N>),
    /// A literal object item.
    LiteralObjectItem(LiteralObjectItem<N>),
    /// A literal output.
    LiteralOutput(LiteralOutput<N>),
    /// A literal output item.
    LiteralOutputItem(LiteralOutputItem<N>),
    /// A literal pair.
    LiteralPair(LiteralPair<N>),
    /// A literal string.
    LiteralString(LiteralString<N>),
    /// A literal struct.
    LiteralStruct(LiteralStruct<N>),
    /// A literal struct item.
    LiteralStructItem(LiteralStructItem<N>),
    /// A logical and expression.
    LogicalAndExpr(LogicalAndExpr<N>),
    /// A logical not expression.
    LogicalNotExpr(LogicalNotExpr<N>),
    /// A logical or expression.
    LogicalOrExpr(LogicalOrExpr<N>),
    /// A map type.
    MapType(MapType<N>),
    /// A metadata array.
    MetadataArray(MetadataArray<N>),
    /// A metadata object.
    MetadataObject(MetadataObject<N>),
    /// A metadata object item.
    MetadataObjectItem(MetadataObjectItem<N>),
    /// A metadata section.
    MetadataSection(MetadataSection<N>),
    /// A modulo expression.
    ModuloExpr(ModuloExpr<N>),
    /// A multiplication expression.
    MultiplicationExpr(MultiplicationExpr<N>),
    /// A reference to a name.
    NameRefExpr(NameRefExpr<N>),
    /// A negation expression.
    NegationExpr(NegationExpr<N>),
    /// An output section.
    OutputSection(OutputSection<N>),
    /// A pair type.
    PairType(PairType<N>),
    /// An object type.
    ObjectType(ObjectType<N>),
    /// A parameter metadata section.
    ParameterMetadataSection(ParameterMetadataSection<N>),
    /// A parenthesized expression.
    ParenthesizedExpr(ParenthesizedExpr<N>),
    /// A placeholder.
    Placeholder(Placeholder<N>),
    /// A primitive type.
    PrimitiveType(PrimitiveType<N>),
    /// A requirements item.
    RequirementsItem(RequirementsItem<N>),
    /// A requirements section.
    RequirementsSection(RequirementsSection<N>),
    /// A runtime item.
    RuntimeItem(RuntimeItem<N>),
    /// A runtime section.
    RuntimeSection(RuntimeSection<N>),
    /// A scatter statement.
    ScatterStatement(ScatterStatement<N>),
    /// The `sep` placeholder option.
    SepOption(SepOption<N>),
    /// A struct definition.
    StructDefinition(StructDefinition<N>),
    /// A subtraction expression.
    SubtractionExpr(SubtractionExpr<N>),
    /// A task definition.
    TaskDefinition(TaskDefinition<N>),
    /// A task item within a hints section.
    TaskHintsItem(TaskHintsItem<N>),
    /// A hints section within a task.
    TaskHintsSection(TaskHintsSection<N>),
    /// A `true`/`false` placeholder option.
    TrueFalseOption(TrueFalseOption<N>),
    /// A reference to a type.
    TypeRef(TypeRef<N>),
    /// An unbound declaration.
    UnboundDecl(UnboundDecl<N>),
    /// A version statement.
    VersionStatement(VersionStatement<N>),
    /// A workflow definition.
    WorkflowDefinition(WorkflowDefinition<N>),
    /// An array within a workflow hints section.
    WorkflowHintsArray(WorkflowHintsArray<N>),
    /// A hints item within a workflow hints section.
    WorkflowHintsItem(WorkflowHintsItem<N>),
    /// An object within a workflow hints section.
    WorkflowHintsObject(WorkflowHintsObject<N>),
    /// An item within an object within a workflow hints section.
    WorkflowHintsObjectItem(WorkflowHintsObjectItem<N>),
    /// A hints section within a workflow.
    WorkflowHintsSection(WorkflowHintsSection<N>),
}

ast_element_impl!(
    Node,
    node,
    TreeNode,
    [
        access_expr(): AccessExprNode => AccessExpr => AccessExpr,
        addition_expr(): AdditionExprNode => AdditionExpr => AdditionExpr,
        array_type(): ArrayTypeNode => ArrayType => ArrayType,
        ast(): RootNode => Ast => Ast,
        bound_decl(): BoundDeclNode => BoundDecl => BoundDecl,
        call_after(): CallAfterNode => CallAfter => CallAfter,
        call_alias(): CallAliasNode => CallAlias => CallAlias,
        call_expr(): CallExprNode => CallExpr => CallExpr,
        call_input_item(): CallInputItemNode => CallInputItem => CallInputItem,
        call_statement(): CallStatementNode => CallStatement => CallStatement,
        call_target(): CallTargetNode => CallTarget => CallTarget,
        command_section(): CommandSectionNode => CommandSection => CommandSection,
        conditional_statement(): ConditionalStatementNode => ConditionalStatement => ConditionalStatement,
        conditional_statement_clause(): ConditionalStatementClauseNode => ConditionalStatementClause => ConditionalStatementClause,
        default_option(): PlaceholderDefaultOptionNode => DefaultOption => DefaultOption,
        division_expr(): DivisionExprNode => DivisionExpr => DivisionExpr,
        equality_expr(): EqualityExprNode => EqualityExpr => EqualityExpr,
        exponentiation_expr(): ExponentiationExprNode => ExponentiationExpr => ExponentiationExpr,
        greater_equal_expr(): GreaterEqualExprNode => GreaterEqualExpr => GreaterEqualExpr,
        greater_expr(): GreaterExprNode => GreaterExpr => GreaterExpr,
        if_expr(): IfExprNode => IfExpr => IfExpr,
        import_alias(): ImportAliasNode => ImportAlias => ImportAlias,
        import_statement(): ImportStatementNode => ImportStatement => ImportStatement,
        index_expr(): IndexExprNode => IndexExpr => IndexExpr,
        inequality_expr(): InequalityExprNode => InequalityExpr => InequalityExpr,
        input_section(): InputSectionNode => InputSection => InputSection,
        less_equal_expr(): LessEqualExprNode => LessEqualExpr => LessEqualExpr,
        less_expr(): LessExprNode => LessExpr => LessExpr,
        literal_array(): LiteralArrayNode => LiteralArray => LiteralArray,
        literal_boolean(): LiteralBooleanNode => LiteralBoolean => LiteralBoolean,
        literal_float(): LiteralFloatNode => LiteralFloat => LiteralFloat,
        literal_hints(): LiteralHintsNode => LiteralHints => LiteralHints,
        literal_hints_item(): LiteralHintsItemNode => LiteralHintsItem => LiteralHintsItem,
        literal_input(): LiteralInputNode => LiteralInput => LiteralInput,
        literal_input_item(): LiteralInputItemNode => LiteralInputItem => LiteralInputItem,
        literal_integer(): LiteralIntegerNode => LiteralInteger => LiteralInteger,
        literal_map(): LiteralMapNode => LiteralMap => LiteralMap,
        literal_map_item(): LiteralMapItemNode => LiteralMapItem => LiteralMapItem,
        literal_none(): LiteralNoneNode => LiteralNone => LiteralNone,
        literal_null(): LiteralNullNode => LiteralNull => LiteralNull,
        literal_object(): LiteralObjectNode => LiteralObject => LiteralObject,
        literal_object_item(): LiteralObjectItemNode => LiteralObjectItem => LiteralObjectItem,
        literal_output(): LiteralOutputNode => LiteralOutput => LiteralOutput,
        literal_output_item(): LiteralOutputItemNode => LiteralOutputItem => LiteralOutputItem,
        literal_pair(): LiteralPairNode => LiteralPair => LiteralPair,
        literal_string(): LiteralStringNode => LiteralString => LiteralString,
        literal_struct(): LiteralStructNode => LiteralStruct => LiteralStruct,
        literal_struct_item(): LiteralStructItemNode => LiteralStructItem => LiteralStructItem,
        logical_and_expr(): LogicalAndExprNode => LogicalAndExpr => LogicalAndExpr,
        logical_not_expr(): LogicalNotExprNode => LogicalNotExpr => LogicalNotExpr,
        logical_or_expr(): LogicalOrExprNode => LogicalOrExpr => LogicalOrExpr,
        map_type(): MapTypeNode => MapType => MapType,
        metadata_array(): MetadataArrayNode => MetadataArray => MetadataArray,
        metadata_object(): MetadataObjectNode => MetadataObject => MetadataObject,
        metadata_object_item(): MetadataObjectItemNode => MetadataObjectItem => MetadataObjectItem,
        metadata_section(): MetadataSectionNode => MetadataSection => MetadataSection,
        modulo_expr(): ModuloExprNode => ModuloExpr => ModuloExpr,
        multiplication_expr(): MultiplicationExprNode => MultiplicationExpr => MultiplicationExpr,
        name_ref_expr(): NameRefExprNode => NameRefExpr => NameRefExpr,
        negation_expr(): NegationExprNode => NegationExpr => NegationExpr,
        object_type(): ObjectTypeNode => ObjectType => ObjectType,
        output_section(): OutputSectionNode => OutputSection => OutputSection,
        pair_type(): PairTypeNode => PairType => PairType,
        parameter_metadata_section(): ParameterMetadataSectionNode => ParameterMetadataSection => ParameterMetadataSection,
        parenthesized_expr(): ParenthesizedExprNode => ParenthesizedExpr => ParenthesizedExpr,
        placeholder(): PlaceholderNode => Placeholder => Placeholder,
        primitive_type(): PrimitiveTypeNode => PrimitiveType => PrimitiveType,
        requirements_item(): RequirementsItemNode => RequirementsItem => RequirementsItem,
        requirements_section(): RequirementsSectionNode => RequirementsSection => RequirementsSection,
        runtime_item(): RuntimeItemNode => RuntimeItem => RuntimeItem,
        runtime_section(): RuntimeSectionNode => RuntimeSection => RuntimeSection,
        scatter_statement(): ScatterStatementNode => ScatterStatement => ScatterStatement,
        sep_option(): PlaceholderSepOptionNode => SepOption => SepOption,
        struct_definition(): StructDefinitionNode => StructDefinition => StructDefinition,
        subtraction_expr(): SubtractionExprNode => SubtractionExpr => SubtractionExpr,
        task_definition(): TaskDefinitionNode => TaskDefinition => TaskDefinition,
        task_hints_item(): TaskHintsItemNode => TaskHintsItem => TaskHintsItem,
        task_hints_section(): TaskHintsSectionNode => TaskHintsSection => TaskHintsSection,
        true_false_option(): PlaceholderTrueFalseOptionNode => TrueFalseOption => TrueFalseOption,
        type_ref(): TypeRefNode => TypeRef => TypeRef,
        unbound_decl(): UnboundDeclNode => UnboundDecl => UnboundDecl,
        version_statement(): VersionStatementNode => VersionStatement => VersionStatement,
        workflow_definition(): WorkflowDefinitionNode => WorkflowDefinition => WorkflowDefinition,
        workflow_hints_array(): WorkflowHintsArrayNode => WorkflowHintsArray => WorkflowHintsArray,
        workflow_hints_item(): WorkflowHintsItemNode => WorkflowHintsItem => WorkflowHintsItem,
        workflow_hints_object(): WorkflowHintsObjectNode => WorkflowHintsObject => WorkflowHintsObject,
        workflow_hints_object_item(): WorkflowHintsObjectItemNode => WorkflowHintsObjectItem => WorkflowHintsObjectItem,
        workflow_hints_section(): WorkflowHintsSectionNode => WorkflowHintsSection => WorkflowHintsSection
    ]
);

/// An abstract syntax tree token.
///
/// This enum has a variant for each struct implementing the [`AstToken`] trait.
#[derive(Clone, Debug)]
pub enum Token<T: TreeToken = SyntaxToken> {
    /// The `after` keyword.
    AfterKeyword(AfterKeyword<T>),
    /// The `alias` keyword.
    AliasKeyword(AliasKeyword<T>),
    /// The `Array` type keyword.
    ArrayTypeKeyword(ArrayTypeKeyword<T>),
    /// The `as` keyword.
    AsKeyword(AsKeyword<T>),
    /// The `=` symbol.
    Assignment(Assignment<T>),
    /// The `*` symbol.
    Asterisk(Asterisk<T>),
    /// The `Boolean` type keyword.
    BooleanTypeKeyword(BooleanTypeKeyword<T>),
    /// The `call` keyword.
    CallKeyword(CallKeyword<T>),
    /// The `}` symbol.
    CloseBrace(CloseBrace<T>),
    /// The `]` symbol.
    CloseBracket(CloseBracket<T>),
    /// The `>>>` symbol.
    CloseHeredoc(CloseHeredoc<T>),
    /// The `)` symbol.
    CloseParen(CloseParen<T>),
    /// The `:` symbol.
    Colon(Colon<T>),
    /// The `,` symbol.
    Comma(Comma<T>),
    /// The `command` keyword.
    CommandKeyword(CommandKeyword<T>),
    /// The text within a command section.
    CommandText(CommandText<T>),
    /// A comment.
    Comment(Comment<T>),
    /// The `Directory` type keyword.
    DirectoryTypeKeyword(DirectoryTypeKeyword<T>),
    /// The `.` symbol.
    Dot(Dot<T>),
    /// The `"` symbol.
    DoubleQuote(DoubleQuote<T>),
    /// The `else` keyword.
    ElseKeyword(ElseKeyword<T>),
    /// The `env` keyword.
    EnvKeyword(EnvKeyword<T>),
    /// The `==` symbol.
    Equal(Equal<T>),
    /// The `!` symbol.
    Exclamation(Exclamation<T>),
    /// The `**` symbol.
    Exponentiation(Exponentiation<T>),
    /// The `false` keyword.
    FalseKeyword(FalseKeyword<T>),
    /// The `File` type keyword.
    FileTypeKeyword(FileTypeKeyword<T>),
    /// A float.
    Float(Float<T>),
    /// The `Float` type keyword.
    FloatTypeKeyword(FloatTypeKeyword<T>),
    /// The `>` symbol.
    Greater(Greater<T>),
    /// The `>=` symbol.
    GreaterEqual(GreaterEqual<T>),
    /// The `hints` keyword.
    HintsKeyword(HintsKeyword<T>),
    /// An identity.
    Ident(Ident<T>),
    /// The `if` keyword.
    IfKeyword(IfKeyword<T>),
    /// The `import` keyword.
    ImportKeyword(ImportKeyword<T>),
    /// The `in` keyword.
    InKeyword(InKeyword<T>),
    /// The `input` keyword.
    InputKeyword(InputKeyword<T>),
    /// An integer.
    Integer(Integer<T>),
    /// The `Int` type keyword.
    IntTypeKeyword(IntTypeKeyword<T>),
    /// The `<` symbol.
    Less(Less<T>),
    /// The `<=` symbol.
    LessEqual(LessEqual<T>),
    /// The `&&` symbol.
    LogicalAnd(LogicalAnd<T>),
    /// The `||` symbol.
    LogicalOr(LogicalOr<T>),
    /// The `Map` type keyword.
    MapTypeKeyword(MapTypeKeyword<T>),
    /// The `meta` keyword.
    MetaKeyword(MetaKeyword<T>),
    /// The `-` symbol.
    Minus(Minus<T>),
    /// The `None` keyword.
    NoneKeyword(NoneKeyword<T>),
    /// The `!=` symbol.
    NotEqual(NotEqual<T>),
    /// The `null` keyword.
    NullKeyword(NullKeyword<T>),
    /// The `object` keyword.
    ObjectKeyword(ObjectKeyword<T>),
    /// The `Object` type keyword.
    ObjectTypeKeyword(ObjectTypeKeyword<T>),
    /// The `{` symbol.
    OpenBrace(OpenBrace<T>),
    /// The `[` symbol.
    OpenBracket(OpenBracket<T>),
    /// The `<<<` symbol.
    OpenHeredoc(OpenHeredoc<T>),
    /// The `(` symbol.
    OpenParen(OpenParen<T>),
    /// The `output` keyword.
    OutputKeyword(OutputKeyword<T>),
    /// The `Pair` type keyword.
    PairTypeKeyword(PairTypeKeyword<T>),
    /// The `parameter_meta` keyword.
    ParameterMetaKeyword(ParameterMetaKeyword<T>),
    /// The `%` symbol.
    Percent(Percent<T>),
    /// One of the placeholder open symbols.
    PlaceholderOpen(PlaceholderOpen<T>),
    /// The `+` symbol.
    Plus(Plus<T>),
    /// The `?` symbol.
    QuestionMark(QuestionMark<T>),
    /// The `requirements` keyword.
    RequirementsKeyword(RequirementsKeyword<T>),
    /// The `runtime` keyword.
    RuntimeKeyword(RuntimeKeyword<T>),
    /// The `scatter` keyword.
    ScatterKeyword(ScatterKeyword<T>),
    /// The `'` symbol.
    SingleQuote(SingleQuote<T>),
    /// The `/` symbol.
    Slash(Slash<T>),
    /// The textual part of a string.
    StringText(StringText<T>),
    /// The `String` type keyword.
    StringTypeKeyword(StringTypeKeyword<T>),
    /// The `struct` keyword.
    StructKeyword(StructKeyword<T>),
    /// The `task` keyword.
    TaskKeyword(TaskKeyword<T>),
    /// The `then` keyword.
    ThenKeyword(ThenKeyword<T>),
    /// The `true` keyword.
    TrueKeyword(TrueKeyword<T>),
    /// A version.
    Version(Version<T>),
    /// The `version` keyword.
    VersionKeyword(VersionKeyword<T>),
    /// Whitespace.
    Whitespace(Whitespace<T>),
    /// The `workflow` keyword.
    WorkflowKeyword(WorkflowKeyword<T>),
}

ast_element_impl!(
    Token,
    token,
    TreeToken,
    [
        after_keyword(): AfterKeyword => AfterKeyword => AfterKeyword,
        alias_keyword(): AliasKeyword => AliasKeyword => AliasKeyword,
        array_type_keyword(): ArrayTypeKeyword => ArrayTypeKeyword => ArrayTypeKeyword,
        as_keyword(): AsKeyword => AsKeyword => AsKeyword,
        assignment(): Assignment => Assignment => Assignment,
        asterisk(): Asterisk => Asterisk => Asterisk,
        boolean_type_keyword(): BooleanTypeKeyword => BooleanTypeKeyword => BooleanTypeKeyword,
        call_keyword(): CallKeyword => CallKeyword => CallKeyword,
        close_brace(): CloseBrace => CloseBrace => CloseBrace,
        close_bracket(): CloseBracket => CloseBracket => CloseBracket,
        close_heredoc(): CloseHeredoc => CloseHeredoc => CloseHeredoc,
        close_paren(): CloseParen => CloseParen => CloseParen,
        colon(): Colon => Colon => Colon,
        comma(): Comma => Comma => Comma,
        command_keyword(): CommandKeyword => CommandKeyword => CommandKeyword,
        command_text(): LiteralCommandText => CommandText => CommandText,
        comment(): Comment => Comment => Comment,
        directory_type_keyword(): DirectoryTypeKeyword => DirectoryTypeKeyword => DirectoryTypeKeyword,
        dot(): Dot => Dot => Dot,
        double_quote(): DoubleQuote => DoubleQuote => DoubleQuote,
        else_keyword(): ElseKeyword => ElseKeyword => ElseKeyword,
        env_keyword(): EnvKeyword => EnvKeyword => EnvKeyword,
        equal(): Equal => Equal => Equal,
        exclamation(): Exclamation => Exclamation => Exclamation,
        exponentiation(): Exponentiation => Exponentiation => Exponentiation,
        false_keyword(): FalseKeyword => FalseKeyword => FalseKeyword,
        file_type_keyword(): FileTypeKeyword => FileTypeKeyword => FileTypeKeyword,
        float(): Float => Float => Float,
        float_type_keyword(): FloatTypeKeyword => FloatTypeKeyword => FloatTypeKeyword,
        greater(): Greater => Greater => Greater,
        greater_equal(): GreaterEqual => GreaterEqual => GreaterEqual,
        hints_keyword(): HintsKeyword => HintsKeyword => HintsKeyword,
        ident(): Ident => Ident => Ident,
        if_keyword(): IfKeyword => IfKeyword => IfKeyword,
        import_keyword(): ImportKeyword => ImportKeyword => ImportKeyword,
        in_keyword(): InKeyword => InKeyword => InKeyword,
        input_keyword(): InputKeyword => InputKeyword => InputKeyword,
        integer(): Integer => Integer => Integer,
        int_type_keyword(): IntTypeKeyword => IntTypeKeyword => IntTypeKeyword,
        less(): Less => Less => Less,
        less_equal(): LessEqual => LessEqual => LessEqual,
        logical_and(): LogicalAnd => LogicalAnd => LogicalAnd,
        logical_or(): LogicalOr => LogicalOr => LogicalOr,
        map_type_keyword(): MapTypeKeyword => MapTypeKeyword => MapTypeKeyword,
        meta_keyword(): MetaKeyword => MetaKeyword => MetaKeyword,
        minus(): Minus => Minus => Minus,
        none_keyword(): NoneKeyword => NoneKeyword => NoneKeyword,
        not_equal(): NotEqual => NotEqual => NotEqual,
        null_keyword(): NullKeyword => NullKeyword => NullKeyword,
        object_keyword(): ObjectKeyword => ObjectKeyword => ObjectKeyword,
        object_type_keyword(): ObjectTypeKeyword => ObjectTypeKeyword => ObjectTypeKeyword,
        open_brace(): OpenBrace => OpenBrace => OpenBrace,
        open_bracket(): OpenBracket => OpenBracket => OpenBracket,
        open_heredoc(): OpenHeredoc => OpenHeredoc => OpenHeredoc,
        open_paren(): OpenParen => OpenParen => OpenParen,
        output_keyword(): OutputKeyword => OutputKeyword => OutputKeyword,
        pair_type_keyword(): PairTypeKeyword => PairTypeKeyword => PairTypeKeyword,
        parameter_meta_keyword(): ParameterMetaKeyword => ParameterMetaKeyword => ParameterMetaKeyword,
        percent(): Percent => Percent => Percent,
        placeholder_open(): PlaceholderOpen => PlaceholderOpen => PlaceholderOpen,
        plus(): Plus => Plus => Plus,
        question_mark(): QuestionMark => QuestionMark => QuestionMark,
        requirements_keyword(): RequirementsKeyword => RequirementsKeyword => RequirementsKeyword,
        runtime_keyword(): RuntimeKeyword => RuntimeKeyword => RuntimeKeyword,
        scatter_keyword(): ScatterKeyword => ScatterKeyword => ScatterKeyword,
        single_quote(): SingleQuote => SingleQuote => SingleQuote,
        slash(): Slash => Slash => Slash,
        string_text(): LiteralStringText => StringText => StringText,
        string_type_keyword(): StringTypeKeyword => StringTypeKeyword => StringTypeKeyword,
        struct_keyword(): StructKeyword => StructKeyword => StructKeyword,
        task_keyword(): TaskKeyword => TaskKeyword => TaskKeyword,
        then_keyword(): ThenKeyword => ThenKeyword => ThenKeyword,
        true_keyword(): TrueKeyword => TrueKeyword => TrueKeyword,
        version_keyword(): VersionKeyword => VersionKeyword => VersionKeyword,
        version(): Version => Version => Version,
        whitespace(): Whitespace => Whitespace => Whitespace,
        workflow_keyword(): WorkflowKeyword => WorkflowKeyword => WorkflowKeyword
    ]
);

/// An abstract syntax tree element.
#[derive(Clone, Debug)]
pub enum Element<N: TreeNode = SyntaxNode> {
    /// An abstract syntax tree node.
    Node(Node<N>),

    /// An abstract syntax tree token.
    Token(Token<N::Token>),
}

impl<N: TreeNode> Element<N> {
    /// Attempts to get a reference to the inner [`Node`].
    ///
    /// * If `self` is a [`Element::Node`], then a reference to the inner
    ///   [`Node`] wrapped in [`Some`] is returned.
    /// * Else, [`None`] is returned.
    pub fn as_node(&self) -> Option<&Node<N>> {
        match self {
            Self::Node(node) => Some(node),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`Node`].
    ///
    /// * If `self` is a [`Element::Node`], then the inner [`Node`] wrapped in
    ///   [`Some`] is returned.
    /// * Else, [`None`] is returned.
    pub fn into_node(self) -> Option<Node<N>> {
        match self {
            Self::Node(node) => Some(node),
            _ => None,
        }
    }

    /// Consumes `self` and returns the inner [`Node`].
    ///
    /// # Panics
    ///
    /// If `self` is not a [`Element::Node`].
    pub fn unwrap_node(self) -> Node<N> {
        self.into_node()
            .expect("expected `Element::Node` but got a different variant")
    }

    /// Attempts to get a reference to the inner [`Token`].
    ///
    /// * If `self` is a [`Element::Token`], then a reference to the inner
    ///   [`Token`] wrapped in [`Some`] is returned.
    /// * Else, [`None`] is returned.
    pub fn as_token(&self) -> Option<&Token<N::Token>> {
        match self {
            Self::Token(token) => Some(token),
            _ => None,
        }
    }

    /// Consumes `self` and attempts to return the inner [`Token`].
    ///
    /// * If `self` is a [`Element::Token`], then the inner [`Token`] wrapped in
    ///   [`Some`] is returned.
    /// * Else, [`None`] is returned.
    pub fn into_token(self) -> Option<Token<N::Token>> {
        match self {
            Self::Token(token) => Some(token),
            _ => None,
        }
    }

    /// Consumes `self` and returns the inner [`Token`].
    ///
    /// # Panics
    ///
    /// If `self` is not a [`Element::Token`].
    pub fn unwrap_token(self) -> Token<N::Token> {
        self.into_token()
            .expect("expected `Element::Token` but got a different variant")
    }

    /// Gets the inner node or token from the [`Element`].
    pub fn inner(&self) -> NodeOrToken<N, N::Token> {
        match self {
            Element::Node(node) => NodeOrToken::Node(node.inner().clone()),
            Element::Token(token) => NodeOrToken::Token(token.inner().clone()),
        }
    }

    /// Gets the underlying [`SyntaxKind`] from the [`Element`].
    pub fn kind(&self) -> SyntaxKind {
        match self {
            Element::Node(node) => node.inner().kind(),
            Element::Token(token) => token.inner().kind(),
        }
    }

    /// Returns whether the [`Element`] represents trivia.
    pub fn is_trivia(&self) -> bool {
        match self {
            Element::Node(node) => node.inner().kind().is_trivia(),
            Element::Token(token) => token.inner().kind().is_trivia(),
        }
    }

    /// Casts an element from a node or a token.
    pub fn cast(element: NodeOrToken<N, N::Token>) -> Self {
        match element {
            NodeOrToken::Node(n) => {
                Self::Node(Node::cast(n).expect("a syntax node should cast to a Node"))
            }
            NodeOrToken::Token(t) => {
                Self::Token(Token::cast(t).expect("a syntax token should cast to a Token"))
            }
        }
    }
}
