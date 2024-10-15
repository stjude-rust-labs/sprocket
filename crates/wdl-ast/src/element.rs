//! Elements (nodes or tokens) within the AST.

use rowan::NodeOrToken;

use crate::AstNode;
use crate::AstToken;
use crate::Comment;
use crate::Ident;
use crate::SyntaxElement;
use crate::SyntaxKind;
use crate::SyntaxNode;
use crate::SyntaxToken;
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
        // The prefix of the syntax element (e.g., `SyntaxNode`).
        $syntax_prefix:ty,
        // A mapping of all of the elements to map from syntax elements to ast
        // elements.
        //
        // E.g., `command_section(): CommandSectionNode => CommandSection => CommandSection`.
        [$($suffix:ident(): $syntax_kind:ty => $inner:ty => $variant:ty),*]
    ) => {
        paste::paste! {
            impl $name {
                #[doc = "Attempts to cast a [`SyntaxElement`] to a [`" $name "`]."]
                pub fn cast(element: SyntaxElement) -> Option<Self> {
                    match element.kind() {
                        $(
                            SyntaxKind::$syntax_kind => {
                                let $display = element
                                    .[<into_ $display>]()
                                    .expect(
                                        "`SyntaxElement` with kind \
                                        `SyntaxKind::${stringify!($syntax_kind)}` could not \
                                        be turned into a `${stringify!($syntax_prefix)}`"
                                    );

                                let inner = $inner::cast($display)
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


                #[doc = "Gets the inner [`" $syntax_prefix "`] from the [`" $name "`]."]
                pub fn syntax(&self) -> &$syntax_prefix {
                    match self {
                        $(
                            $name::$variant(inner) => inner.syntax(),
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
                    pub fn [<as_ $suffix>](&self) -> Option<&$inner> {
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
                    pub fn [<into_ $suffix>](self) -> Option<$inner> {
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
                    pub fn [<unwrap_ $suffix>](self) -> $inner {
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
pub enum Node {
    /// An access expression.
    AccessExpr(AccessExpr),
    /// An addition expression.
    AdditionExpr(AdditionExpr),
    /// An array type.
    ArrayType(ArrayType),
    /// A V1 abstract syntax tree.
    Ast(Ast),
    /// A bound declaration.
    BoundDecl(BoundDecl),
    /// An after clause in a call statement.
    CallAfter(CallAfter),
    /// An alias clause in a call statement.
    CallAlias(CallAlias),
    /// A call expression.
    CallExpr(CallExpr),
    /// A call input item.
    CallInputItem(CallInputItem),
    /// A call statement.
    CallStatement(CallStatement),
    /// A target within a call statement.
    CallTarget(CallTarget),
    /// A command section.
    CommandSection(CommandSection),
    /// A conditional statement.
    ConditionalStatement(ConditionalStatement),
    /// The `default` placeholder option.
    DefaultOption(DefaultOption),
    /// A division expression.
    DivisionExpr(DivisionExpr),
    /// An equality expression.
    EqualityExpr(EqualityExpr),
    /// An exponentiation expression.
    ExponentiationExpr(ExponentiationExpr),
    /// A greater than or equal to expression.
    GreaterEqualExpr(GreaterEqualExpr),
    /// A greater than expression.
    GreaterExpr(GreaterExpr),
    /// An if expression.
    IfExpr(IfExpr),
    /// An import alias.
    ImportAlias(ImportAlias),
    /// An import statement.
    ImportStatement(ImportStatement),
    /// An index expression.
    IndexExpr(IndexExpr),
    /// An inequality expression.
    InequalityExpr(InequalityExpr),
    /// An input section.
    InputSection(InputSection),
    /// A less than or equal to expression.
    LessEqualExpr(LessEqualExpr),
    /// A less than expression.
    LessExpr(LessExpr),
    /// A literal array.
    LiteralArray(LiteralArray),
    /// A literal boolean.
    LiteralBoolean(LiteralBoolean),
    /// A literal float.
    LiteralFloat(LiteralFloat),
    /// A literal hints.
    LiteralHints(LiteralHints),
    /// A literal hints item.
    LiteralHintsItem(LiteralHintsItem),
    /// A literal input.
    LiteralInput(LiteralInput),
    /// A literal input item.
    LiteralInputItem(LiteralInputItem),
    /// A literal integer.
    LiteralInteger(LiteralInteger),
    /// A literal map.
    LiteralMap(LiteralMap),
    /// A literal map item.
    LiteralMapItem(LiteralMapItem),
    /// A literal none.
    LiteralNone(LiteralNone),
    /// A literal null.
    LiteralNull(LiteralNull),
    /// A literal object.
    LiteralObject(LiteralObject),
    /// A literal object item.
    LiteralObjectItem(LiteralObjectItem),
    /// A literal output.
    LiteralOutput(LiteralOutput),
    /// A literal output item.
    LiteralOutputItem(LiteralOutputItem),
    /// A literal pair.
    LiteralPair(LiteralPair),
    /// A literal string.
    LiteralString(LiteralString),
    /// A literal struct.
    LiteralStruct(LiteralStruct),
    /// A literal struct item.
    LiteralStructItem(LiteralStructItem),
    /// A logical and expression.
    LogicalAndExpr(LogicalAndExpr),
    /// A logical not expression.
    LogicalNotExpr(LogicalNotExpr),
    /// A logical or expression.
    LogicalOrExpr(LogicalOrExpr),
    /// A map type.
    MapType(MapType),
    /// A metadata array.
    MetadataArray(MetadataArray),
    /// A metadata object.
    MetadataObject(MetadataObject),
    /// A metadata object item.
    MetadataObjectItem(MetadataObjectItem),
    /// A metadata section.
    MetadataSection(MetadataSection),
    /// A modulo expression.
    ModuloExpr(ModuloExpr),
    /// A multiplication expression.
    MultiplicationExpr(MultiplicationExpr),
    /// A reference to a name.
    NameRef(NameRef),
    /// A negation expression.
    NegationExpr(NegationExpr),
    /// An output section.
    OutputSection(OutputSection),
    /// A pair type.
    PairType(PairType),
    /// An object type.
    ObjectType(ObjectType),
    /// A parameter metadata section.
    ParameterMetadataSection(ParameterMetadataSection),
    /// A parenthesized expression.
    ParenthesizedExpr(ParenthesizedExpr),
    /// A placeholder.
    Placeholder(Placeholder),
    /// A primitive type.
    PrimitiveType(PrimitiveType),
    /// A requirements item.
    RequirementsItem(RequirementsItem),
    /// A requirements section.
    RequirementsSection(RequirementsSection),
    /// A runtime item.
    RuntimeItem(RuntimeItem),
    /// A runtime section.
    RuntimeSection(RuntimeSection),
    /// A scatter statement.
    ScatterStatement(ScatterStatement),
    /// The `sep` placeholder option.
    SepOption(SepOption),
    /// A struct definition.
    StructDefinition(StructDefinition),
    /// A subtraction expression.
    SubtractionExpr(SubtractionExpr),
    /// A task definition.
    TaskDefinition(TaskDefinition),
    /// A task item within a hints section.
    TaskHintsItem(TaskHintsItem),
    /// A hints section within a task.
    TaskHintsSection(TaskHintsSection),
    /// A `true`/`false` placeholder option.
    TrueFalseOption(TrueFalseOption),
    /// A reference to a type.
    TypeRef(TypeRef),
    /// An unbound declaration.
    UnboundDecl(UnboundDecl),
    /// A version statement.
    VersionStatement(VersionStatement),
    /// A workflow definition.
    WorkflowDefinition(WorkflowDefinition),
    /// An array within a workflow hints section.
    WorkflowHintsArray(WorkflowHintsArray),
    /// A hints item within a workflow hints section.
    WorkflowHintsItem(WorkflowHintsItem),
    /// An object within a workflow hints section.
    WorkflowHintsObject(WorkflowHintsObject),
    /// An item within an object within a workflow hints section.
    WorkflowHintsObjectItem(WorkflowHintsObjectItem),
    /// A hints section within a workflow.
    WorkflowHintsSection(WorkflowHintsSection),
}

ast_element_impl!(
    Node,
    node,
    SyntaxNode,
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
        name_ref(): NameRefNode => NameRef => NameRef,
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
pub enum Token {
    /// The `after` keyword.
    AfterKeyword(AfterKeyword),
    /// The `alias` keyword.
    AliasKeyword(AliasKeyword),
    /// The `Array` type keyword.
    ArrayTypeKeyword(ArrayTypeKeyword),
    /// The `as` keyword.
    AsKeyword(AsKeyword),
    /// The `=` symbol.
    Assignment(Assignment),
    /// The `*` symbol.
    Asterisk(Asterisk),
    /// The `Boolean` type keyword.
    BooleanTypeKeyword(BooleanTypeKeyword),
    /// The `call` keyword.
    CallKeyword(CallKeyword),
    /// The `}` symbol.
    CloseBrace(CloseBrace),
    /// The `]` symbol.
    CloseBracket(CloseBracket),
    /// The `>>>` symbol.
    CloseHeredoc(CloseHeredoc),
    /// The `)` symbol.
    CloseParen(CloseParen),
    /// The `:` symbol.
    Colon(Colon),
    /// The `,` symbol.
    Comma(Comma),
    /// The `command` keyword.
    CommandKeyword(CommandKeyword),
    /// The text within a command section.
    CommandText(CommandText),
    /// A comment.
    Comment(Comment),
    /// The `Directory` type keyword.
    DirectoryTypeKeyword(DirectoryTypeKeyword),
    /// The `.` symbol.
    Dot(Dot),
    /// The `"` symbol.
    DoubleQuote(DoubleQuote),
    /// The `else` keyword.
    ElseKeyword(ElseKeyword),
    /// The `==` symbol.
    Equal(Equal),
    /// The `!` symbol.
    Exclamation(Exclamation),
    /// The `**` symbol.
    Exponentiation(Exponentiation),
    /// The `false` keyword.
    FalseKeyword(FalseKeyword),
    /// The `File` type keyword.
    FileTypeKeyword(FileTypeKeyword),
    /// A float.
    Float(Float),
    /// The `Float` type keyword.
    FloatTypeKeyword(FloatTypeKeyword),
    /// The `>` symbol.
    Greater(Greater),
    /// The `>=` symbol.
    GreaterEqual(GreaterEqual),
    /// The `hints` keyword.
    HintsKeyword(HintsKeyword),
    /// An identity.
    Ident(Ident),
    /// The `if` keyword.
    IfKeyword(IfKeyword),
    /// The `import` keyword.
    ImportKeyword(ImportKeyword),
    /// The `in` keyword.
    InKeyword(InKeyword),
    /// The `input` keyword.
    InputKeyword(InputKeyword),
    /// An integer.
    Integer(Integer),
    /// The `Int` type keyword.
    IntTypeKeyword(IntTypeKeyword),
    /// The `<` symbol.
    Less(Less),
    /// The `<=` symbol.
    LessEqual(LessEqual),
    /// The `&&` symbol.
    LogicalAnd(LogicalAnd),
    /// The `||` symbol.
    LogicalOr(LogicalOr),
    /// The `Map` type keyword.
    MapTypeKeyword(MapTypeKeyword),
    /// The `meta` keyword.
    MetaKeyword(MetaKeyword),
    /// The `-` symbol.
    Minus(Minus),
    /// The `None` keyword.
    NoneKeyword(NoneKeyword),
    /// The `!=` symbol.
    NotEqual(NotEqual),
    /// The `null` keyword.
    NullKeyword(NullKeyword),
    /// The `object` keyword.
    ObjectKeyword(ObjectKeyword),
    /// The `Object` type keyword.
    ObjectTypeKeyword(ObjectTypeKeyword),
    /// The `{` symbol.
    OpenBrace(OpenBrace),
    /// The `[` symbol.
    OpenBracket(OpenBracket),
    /// The `<<<` symbol.
    OpenHeredoc(OpenHeredoc),
    /// The `(` symbol.
    OpenParen(OpenParen),
    /// The `output` keyword.
    OutputKeyword(OutputKeyword),
    /// The `Pair` type keyword.
    PairTypeKeyword(PairTypeKeyword),
    /// The `parameter_meta` keyword.
    ParameterMetaKeyword(ParameterMetaKeyword),
    /// The `%` symbol.
    Percent(Percent),
    /// One of the placeholder open symbols.
    PlaceholderOpen(PlaceholderOpen),
    /// The `+` symbol.
    Plus(Plus),
    /// The `?` symbol.
    QuestionMark(QuestionMark),
    /// The `requirements` keyword.
    RequirementsKeyword(RequirementsKeyword),
    /// The `runtime` keyword.
    RuntimeKeyword(RuntimeKeyword),
    /// The `scatter` keyword.
    ScatterKeyword(ScatterKeyword),
    /// The `'` symbol.
    SingleQuote(SingleQuote),
    /// The `/` symbol.
    Slash(Slash),
    /// The textual part of a string.
    StringText(StringText),
    /// The `String` type keyword.
    StringTypeKeyword(StringTypeKeyword),
    /// The `struct` keyword.
    StructKeyword(StructKeyword),
    /// The `task` keyword.
    TaskKeyword(TaskKeyword),
    /// The `then` keyword.
    ThenKeyword(ThenKeyword),
    /// The `true` keyword.
    TrueKeyword(TrueKeyword),
    /// A version.
    Version(Version),
    /// The `version` keyword.
    VersionKeyword(VersionKeyword),
    /// Whitespace.
    Whitespace(Whitespace),
    /// The `workflow` keyword.
    WorkflowKeyword(WorkflowKeyword),
}

ast_element_impl!(
    Token,
    token,
    SyntaxToken,
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
        close_brack(): CloseBracket => CloseBracket => CloseBracket,
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
        equal(): Equal => Equal => Equal,
        exclaimation(): Exclamation => Exclamation => Exclamation,
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
pub enum Element {
    /// An abstract syntax tree node.
    Node(Node),

    /// An abstract syntax tree token.
    Token(Token),
}

impl Element {
    /// Attempts to get a reference to the inner [`Node`].
    ///
    /// * If `self` is a [`Element::Node`], then a reference to the inner
    ///   [`Node`] wrapped in [`Some`] is returned.
    /// * Else, [`None`] is returned.
    pub fn as_node(&self) -> Option<&Node> {
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
    pub fn into_node(self) -> Option<Node> {
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
    pub fn unwrap_node(self) -> Node {
        self.into_node()
            .expect("expected `Element::Node` but got a different variant")
    }

    /// Attempts to get a reference to the inner [`Token`].
    ///
    /// * If `self` is a [`Element::Token`], then a reference to the inner
    ///   [`Token`] wrapped in [`Some`] is returned.
    /// * Else, [`None`] is returned.
    pub fn as_token(&self) -> Option<&Token> {
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
    pub fn into_token(self) -> Option<Token> {
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
    pub fn unwrap_token(self) -> Token {
        self.into_token()
            .expect("expected `Element::Token` but got a different variant")
    }

    /// Gets the underlying [`SyntaxElement`] from the [`Element`].
    pub fn syntax(&self) -> SyntaxElement {
        match self {
            Element::Node(node) => SyntaxElement::Node(node.syntax().clone()),
            Element::Token(token) => SyntaxElement::Token(token.syntax().clone()),
        }
    }

    /// Gets the underlying [`SyntaxKind`] from the [`Element`].
    pub fn kind(&self) -> SyntaxKind {
        match self {
            Element::Node(node) => node.syntax().kind(),
            Element::Token(token) => token.syntax().kind(),
        }
    }

    /// Returns whether the [`SyntaxElement`] represents trivia.
    pub fn is_trivia(&self) -> bool {
        match self {
            Element::Node(node) => node.syntax().kind().is_trivia(),
            Element::Token(token) => token.syntax().kind().is_trivia(),
        }
    }

    /// Casts a [`SyntaxElement`] to an [`Element`].
    ///
    /// This is expected to always succeed, as any [`SyntaxElement`] _should_
    /// have a corresponding [`Element`] (and, if it doesn't, that's very
    /// likely a bug).
    pub fn cast(element: SyntaxElement) -> Self {
        match &element {
            NodeOrToken::Node(_) => {
                Self::Node(Node::cast(element).expect("a syntax node should cast to a Node"))
            }
            NodeOrToken::Token(_) => {
                Self::Token(Token::cast(element).expect("a syntax token should cast to a Token"))
            }
        }
    }
}
