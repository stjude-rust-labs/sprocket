//! A lightweight model of how Bash tokenizes command text.
//!
//! The model is only precise enough to decide whether a value inserted at a
//! given position in a script is subject to word splitting and pathname
//! expansion. It is not a Bash parser.

/// Reserved words after which the next word is still in command position.
const COMMAND_KEYWORDS: &[&str] = &[
    "!", "{", "do", "elif", "else", "if", "then", "time", "until", "while",
];

/// Reserved words that end a compound command.
const CLOSING_KEYWORDS: &[&str] = &["}", "done", "esac", "fi"];

/// Builtins whose `name=value` arguments are assignments.
const DECLARATION_BUILTINS: &[&str] = &["declare", "export", "local", "readonly", "typeset"];

/// How a frame of unquoted shell code ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum CodeEnd {
    /// The end of the script.
    Script,
    /// A closing parenthesis (subshells, command and process substitutions,
    /// and array assignments).
    Paren,
    /// A closing backtick.
    Backtick,
    /// A closing `]]`.
    DoubleBracket,
}

/// The syntactic position of a word in unquoted shell code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum WordPos {
    /// The start of a command, where assignments and reserved words may occur.
    Command,
    /// A command argument.
    Argument,
    /// An argument to a declaration builtin, such as `export`.
    Declaration,
    /// The variable name of a `for` or `select` loop.
    LoopName,
    /// The word after a loop variable name, which may be `in`.
    LoopIn,
    /// The word list of a `for` or `select` loop.
    LoopList,
    /// The word of a `case` statement.
    CaseWord,
    /// The word after a `case` word, which should be `in`.
    CaseIn,
    /// A `case` pattern.
    CasePattern,
}

/// A frame of unquoted shell code.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Code {
    /// How the frame ends.
    end: CodeEnd,
    /// The position of the current or next word.
    pos: WordPos,
    /// The text of the current word, if a word is in progress.
    word: Option<String>,
    /// Whether the current word only contains unquoted literal characters.
    plain: bool,
    /// Whether the current word is the value of an assignment.
    assignment: bool,
    /// Whether the current word is the target of a redirection.
    redirect: bool,
    /// Whether the next word is the target of a redirection.
    redirect_next: bool,
    /// The number of `case` statements open in this frame.
    cases: u32,
}

impl Code {
    /// Creates a new frame of shell code.
    fn new(end: CodeEnd, pos: WordPos) -> Self {
        Self {
            end,
            pos,
            word: None,
            plain: true,
            assignment: false,
            redirect: false,
            redirect_next: false,
            cases: 0,
        }
    }

    /// Starts a word if one is not already in progress.
    fn start_word(&mut self) {
        if self.word.is_none() {
            self.word = Some(String::new());
            self.plain = true;
            self.assignment = false;
            self.redirect = self.redirect_next;
            self.redirect_next = false;
        }
    }

    /// Adds a character to the current word.
    fn push(&mut self, c: char) {
        self.start_word();
        if let Some(word) = &mut self.word {
            word.push(c);
        }
    }

    /// Adds a quoted or expanded segment to the current word.
    fn push_special(&mut self) {
        self.start_word();

        // Quotes and expansions in an array subscript, such as `a["key"]=`,
        // do not prevent the word from being an assignment.
        let in_subscript = self
            .word
            .as_deref()
            .is_some_and(|w| w.contains('[') && !w.contains(']'));
        if !in_subscript {
            self.plain = false;
        }
    }

    /// Discards the current word if it is the file descriptor of a
    /// redirection, such as the `2` in `2>file`.
    fn discard_io_number(&mut self) {
        if self.plain
            && !self.redirect
            && self
                .word
                .as_deref()
                .is_some_and(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_digit()))
        {
            self.word = None;
            self.plain = true;
            self.assignment = false;
        }
    }

    /// Handles an `=` in the current word.
    fn push_equals(&mut self) {
        self.start_word();
        let eligible = matches!(self.pos, WordPos::Command | WordPos::Declaration);
        if eligible
            && self.plain
            && !self.assignment
            && !self.redirect
            && self.word.as_deref().is_some_and(is_assignment_name)
        {
            self.assignment = true;
        }
        self.push('=');
    }

    /// Ends the current word, if any, and updates the word position.
    fn end_word(&mut self) {
        let Some(word) = self.word.take() else {
            return;
        };

        let keyword = (self.plain && !self.assignment).then_some(word.as_str());
        if self.redirect || (self.assignment && self.pos == WordPos::Command) {
            // Redirections and assignments before a command do not change the
            // position of the next word.
        } else {
            self.pos = match (self.pos, keyword) {
                (WordPos::Command, Some("for" | "select")) => WordPos::LoopName,
                (WordPos::Command, Some("case")) => WordPos::CaseWord,
                (WordPos::Command | WordPos::CasePattern, Some("esac")) if self.cases > 0 => {
                    self.cases -= 1;
                    WordPos::Argument
                }
                (WordPos::Command, Some(k)) if COMMAND_KEYWORDS.contains(&k) => WordPos::Command,
                (WordPos::Command, Some(k)) if CLOSING_KEYWORDS.contains(&k) => WordPos::Argument,
                (WordPos::Command, Some(k)) if DECLARATION_BUILTINS.contains(&k) => {
                    WordPos::Declaration
                }
                (WordPos::Command, _) => WordPos::Argument,
                (WordPos::LoopName, _) => WordPos::LoopIn,
                (WordPos::LoopIn, Some("in")) => WordPos::LoopList,
                (WordPos::LoopIn, Some("do")) => WordPos::Command,
                (WordPos::LoopIn, _) => WordPos::Argument,
                (WordPos::CaseWord, _) => WordPos::CaseIn,
                (WordPos::CaseIn, Some("in")) => {
                    self.cases += 1;
                    WordPos::CasePattern
                }
                (WordPos::CaseIn, _) => WordPos::Argument,
                (pos, _) => pos,
            };
        }

        self.plain = true;
        self.assignment = false;
        self.redirect = false;
    }

    /// Handles the end of a line.
    fn newline(&mut self) {
        self.end_word();
        self.redirect_next = false;
        self.pos = match self.pos {
            WordPos::CaseWord | WordPos::CaseIn | WordPos::CasePattern => self.pos,
            _ => WordPos::Command,
        };
    }

    /// Handles a command separator such as `;`, `&&`, or `|`.
    fn separator(&mut self) {
        self.end_word();
        self.redirect_next = false;
        self.pos = WordPos::Command;
    }

    /// Determines whether a value inserted at the current position is split.
    fn splits(&self) -> bool {
        if self.end == CodeEnd::DoubleBracket || self.assignment {
            return false;
        }

        !matches!(
            self.pos,
            WordPos::LoopName
                | WordPos::LoopIn
                | WordPos::CaseWord
                | WordPos::CaseIn
                | WordPos::CasePattern
        )
    }
}

/// Determines whether a word is a valid assignment target, such as `name`,
/// `name+`, or `name[index]`.
fn is_assignment_name(word: &str) -> bool {
    let word = word.strip_suffix('+').unwrap_or(word);
    let name = match word.find('[') {
        Some(index) if word.ends_with(']') => &word[..index],
        Some(_) => return false,
        None => word,
    };

    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A heredoc whose body has not ended.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct HereDoc {
    /// The delimiter that ends the heredoc.
    delimiter: String,
    /// Whether leading tabs are stripped from each line (`<<-`).
    strip_tabs: bool,
}

impl HereDoc {
    /// Determines whether a line of the body ends the heredoc.
    fn ends_with(&self, line: &str) -> bool {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let line = if self.strip_tabs {
            line.trim_start_matches('\t')
        } else {
            line
        };
        line == self.delimiter
    }
}

/// A frame in the shell state.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Frame {
    /// Unquoted shell code.
    Code(Code),
    /// A single-quoted string.
    Single,
    /// An ANSI-C quoted string (`$'...'`).
    AnsiC,
    /// A double-quoted string.
    Double,
    /// A parameter expansion (`${...}`) with the given brace depth.
    Parameter(u32),
    /// An arithmetic expression (`$((...))` or `((...))`) with the given
    /// parenthesis depth.
    Arithmetic(u32),
    /// A heredoc body.
    HereDoc {
        /// The heredoc to read.
        heredoc: HereDoc,
        /// The text of the current line, or `None` if the line contains an
        /// inserted value.
        line: Option<String>,
    },
    /// A comment.
    Comment,
}

/// The state of the shell tokenizer at a position in a script.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ShellState {
    /// The stack of open frames.
    frames: Vec<Frame>,
    /// The heredocs whose bodies begin on the next line.
    heredocs: Vec<HereDoc>,
}

impl Default for ShellState {
    fn default() -> Self {
        Self {
            frames: vec![Frame::Code(Code::new(CodeEnd::Script, WordPos::Command))],
            heredocs: Vec::new(),
        }
    }
}

impl ShellState {
    /// Feeds literal script text to the state.
    pub fn feed(&mut self, text: &str) {
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            i = self.step(&chars, i);
        }
    }

    /// Inserts a value that is not known until runtime.
    ///
    /// Returns `true` if Bash would split the value into words.
    pub fn insert(&mut self) -> bool {
        match self.frames.last_mut() {
            Some(Frame::Code(code)) => code.push_special(),
            Some(Frame::HereDoc { line, .. }) => *line = None,
            _ => {}
        }

        for frame in self.frames.iter().rev() {
            match frame {
                Frame::Parameter(_) => continue,
                Frame::Code(code) => return code.splits(),
                _ => return false,
            }
        }

        false
    }

    /// Determines whether the current position is inside a single-quoted or
    /// ANSI-C quoted string, where expansions are not performed.
    pub fn in_single_quotes(&self) -> bool {
        matches!(self.frames.last(), Some(Frame::Single | Frame::AnsiC))
    }

    /// Gets the innermost frame of shell code.
    fn code(&mut self) -> &mut Code {
        match self.frames.last_mut() {
            Some(Frame::Code(code)) => code,
            _ => unreachable!("the innermost frame should be code"),
        }
    }

    /// Pops the innermost frame, never popping the outermost frame.
    fn pop(&mut self) {
        if self.frames.len() > 1 {
            self.frames.pop();
        }
    }

    /// Pushes a new frame of shell code.
    fn push_code(&mut self, end: CodeEnd, pos: WordPos) {
        self.frames.push(Frame::Code(Code::new(end, pos)));
    }

    /// Pushes a frame for the body of the next pending heredoc, if any.
    fn push_heredoc(&mut self) {
        if !self.heredocs.is_empty() {
            let heredoc = self.heredocs.remove(0);
            self.frames.push(Frame::HereDoc {
                heredoc,
                line: Some(String::new()),
            });
        }
    }

    /// Processes the character at index `i` and returns the next index.
    fn step(&mut self, chars: &[char], i: usize) -> usize {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match self.frames.last_mut().expect("there should be a frame") {
            Frame::Code(_) => self.step_code(chars, i),
            Frame::Single => {
                if c == '\'' {
                    self.pop();
                }
                i + 1
            }
            Frame::AnsiC => match c {
                '\\' => i + 2,
                '\'' => {
                    self.pop();
                    i + 1
                }
                _ => i + 1,
            },
            Frame::Double => match c {
                '"' => {
                    self.pop();
                    i + 1
                }
                '\\' => i + 2,
                '$' => self.dollar(chars, i),
                '`' => {
                    self.push_code(CodeEnd::Backtick, WordPos::Command);
                    i + 1
                }
                _ => i + 1,
            },
            Frame::Parameter(depth) => match c {
                '}' if *depth == 0 => {
                    self.pop();
                    i + 1
                }
                '}' => {
                    *depth -= 1;
                    i + 1
                }
                '{' => {
                    *depth += 1;
                    i + 1
                }
                '\\' => i + 2,
                '\'' => {
                    self.frames.push(Frame::Single);
                    i + 1
                }
                '"' => {
                    self.frames.push(Frame::Double);
                    i + 1
                }
                '$' => self.dollar(chars, i),
                _ => i + 1,
            },
            Frame::Arithmetic(depth) => match c {
                '(' => {
                    *depth += 1;
                    i + 1
                }
                ')' if *depth == 0 => {
                    self.pop();
                    if next == Some(')') { i + 2 } else { i + 1 }
                }
                ')' => {
                    *depth -= 1;
                    i + 1
                }
                '$' => self.dollar(chars, i),
                _ => i + 1,
            },
            Frame::HereDoc { heredoc, line } => {
                if c == '\n' {
                    let ended = line.as_deref().is_some_and(|l| heredoc.ends_with(l));
                    *line = Some(String::new());
                    if ended {
                        self.pop();
                        self.push_heredoc();
                    }
                } else if let Some(line) = line {
                    line.push(c);
                }
                i + 1
            }
            Frame::Comment => {
                if c == '\n' {
                    // Let the enclosing code handle the newline.
                    self.pop();
                    i
                } else {
                    i + 1
                }
            }
        }
    }

    /// Processes a `$` at index `i` and returns the next index.
    fn dollar(&mut self, chars: &[char], i: usize) -> usize {
        let in_code = matches!(self.frames.last(), Some(Frame::Code(_)));
        match (chars.get(i + 1), chars.get(i + 2)) {
            (Some('('), Some('(')) => {
                self.frames.push(Frame::Arithmetic(0));
                i + 3
            }
            (Some('('), _) => {
                self.push_code(CodeEnd::Paren, WordPos::Command);
                i + 2
            }
            (Some('{'), _) => {
                self.frames.push(Frame::Parameter(0));
                i + 2
            }
            (Some('\''), _) if in_code => {
                self.frames.push(Frame::AnsiC);
                i + 2
            }
            (Some('"'), _) if in_code => {
                self.frames.push(Frame::Double);
                i + 2
            }
            (Some('#' | '?' | '$' | '!' | '@' | '*' | '-' | '0'..='9'), _) => i + 2,
            _ => i + 1,
        }
    }

    /// Processes a character in unquoted shell code and returns the next
    /// index.
    fn step_code(&mut self, chars: &[char], i: usize) -> usize {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        let boundary = |index: usize| {
            chars
                .get(index)
                .is_none_or(|c| c.is_whitespace() || matches!(c, ';' | '&' | '|' | ')'))
        };

        let code = self.code();
        match c {
            ' ' | '\t' | '\r' => {
                code.end_word();
                i + 1
            }
            '\n' => {
                code.newline();
                self.push_heredoc();
                i + 1
            }
            ';' if matches!(next, Some(';' | '&')) => {
                // A `case` clause terminator (`;;`, `;&`, or `;;&`).
                code.separator();
                if code.cases > 0 {
                    code.pos = WordPos::CasePattern;
                }
                if next == Some(';') && chars.get(i + 2) == Some(&'&') {
                    i + 3
                } else {
                    i + 2
                }
            }
            ';' => {
                code.separator();
                i + 1
            }
            '&' if next == Some('>') => {
                code.discard_io_number();
                code.end_word();
                self.redirect(chars, i + 1)
            }
            '&' => {
                code.separator();
                if next == Some('&') { i + 2 } else { i + 1 }
            }
            '|' => {
                let pattern = code.pos == WordPos::CasePattern;
                code.separator();
                if pattern {
                    code.pos = WordPos::CasePattern;
                }
                if matches!(next, Some('|' | '&')) {
                    i + 2
                } else {
                    i + 1
                }
            }
            '(' => {
                if code.assignment && code.word.as_deref().is_some_and(|w| w.ends_with('=')) {
                    // An array assignment.
                    self.push_code(CodeEnd::Paren, WordPos::Argument);
                } else if code.word.is_none()
                    && matches!(code.pos, WordPos::Command | WordPos::LoopName)
                    && next == Some('(')
                {
                    // An arithmetic command or a C-style `for` loop.
                    code.pos = WordPos::Argument;
                    self.frames.push(Frame::Arithmetic(0));
                    return i + 2;
                } else if code.word.is_none() && code.pos == WordPos::CasePattern {
                    // The optional opening parenthesis of a `case` pattern.
                } else {
                    code.end_word();
                    self.push_code(CodeEnd::Paren, WordPos::Command);
                }
                i + 1
            }
            ')' => {
                code.end_word();
                if code.pos == WordPos::CasePattern && code.cases > 0 {
                    code.pos = WordPos::Command;
                } else if code.end == CodeEnd::Paren {
                    self.pop();
                }
                i + 1
            }
            '`' => {
                if code.end == CodeEnd::Backtick {
                    code.end_word();
                    self.pop();
                } else {
                    code.push_special();
                    self.push_code(CodeEnd::Backtick, WordPos::Command);
                }
                i + 1
            }
            '<' | '>' if next == Some('(') => {
                // A process substitution.
                code.push_special();
                self.push_code(CodeEnd::Paren, WordPos::Command);
                i + 2
            }
            '<' if next == Some('<') && chars.get(i + 2) == Some(&'<') => {
                // A here string.
                code.discard_io_number();
                code.end_word();
                code.redirect_next = true;
                i + 3
            }
            '<' if next == Some('<') => {
                code.discard_io_number();
                code.end_word();
                self.heredoc(chars, i + 2)
            }
            '<' | '>' => {
                code.discard_io_number();
                code.end_word();
                self.redirect(chars, i)
            }
            '#' if code.word.is_none() => {
                self.frames.push(Frame::Comment);
                i + 1
            }
            '\\' => {
                if next != Some('\n') {
                    code.push_special();
                }
                i + 2
            }
            '\'' => {
                code.push_special();
                self.frames.push(Frame::Single);
                i + 1
            }
            '"' => {
                code.push_special();
                self.frames.push(Frame::Double);
                i + 1
            }
            '$' => {
                code.push_special();
                self.dollar(chars, i)
            }
            '[' if code.word.is_none()
                && code.pos == WordPos::Command
                && next == Some('[')
                && boundary(i + 2) =>
            {
                code.pos = WordPos::Argument;
                self.push_code(CodeEnd::DoubleBracket, WordPos::Argument);
                i + 2
            }
            ']' if code.end == CodeEnd::DoubleBracket
                && code.word.is_none()
                && next == Some(']')
                && boundary(i + 2) =>
            {
                self.pop();
                i + 2
            }
            '=' => {
                code.push_equals();
                i + 1
            }
            _ => {
                code.push(c);
                i + 1
            }
        }
    }

    /// Processes a redirection operator starting at index `i` and returns the
    /// next index.
    fn redirect(&mut self, chars: &[char], mut i: usize) -> usize {
        while chars.get(i).is_some_and(|c| matches!(c, '<' | '>')) {
            i += 1;
        }

        if chars.get(i).is_some_and(|c| matches!(c, '&' | '|')) {
            i += 1;
        }

        self.code().redirect_next = true;
        i
    }

    /// Processes a heredoc delimiter starting at index `i` (after `<<`) and
    /// returns the next index.
    fn heredoc(&mut self, chars: &[char], mut i: usize) -> usize {
        let strip_tabs = chars.get(i) == Some(&'-');
        if strip_tabs {
            i += 1;
        }

        while chars.get(i).is_some_and(|c| matches!(c, ' ' | '\t')) {
            i += 1;
        }

        // Read the delimiter as a shell word, removing quotes.
        let mut delimiter = String::new();
        let mut quote = None;
        while let Some(&c) = chars.get(i) {
            match (quote, c) {
                (Some(q), c) if c == q => quote = None,
                (Some('"'), '\\') => {
                    i += 1;
                    if let Some(&c) = chars.get(i) {
                        delimiter.push(c);
                    }
                }
                (Some(_), c) => delimiter.push(c),
                (None, '\'' | '"') => quote = Some(c),
                (None, '\\') => {
                    i += 1;
                    if let Some(&c) = chars.get(i) {
                        delimiter.push(c);
                    }
                }
                (None, c)
                    if c.is_whitespace()
                        || matches!(c, ';' | '&' | '|' | '<' | '>' | '(' | ')') =>
                {
                    break;
                }
                (None, c) => delimiter.push(c),
            }

            i += 1;
        }

        if !delimiter.is_empty() {
            self.heredocs.push(HereDoc {
                delimiter,
                strip_tabs,
            });
        }

        i
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    /// Feeds a script where each `@` marks an inserted value and returns
    /// whether each inserted value is split.
    fn splits(script: &str) -> Vec<bool> {
        let mut state = ShellState::default();
        let mut results = Vec::new();
        let mut parts = script.split('@');
        state.feed(parts.next().unwrap());
        for part in parts {
            results.push(state.insert());
            state.feed(part);
        }
        results
    }

    #[test]
    fn arguments_are_split() {
        assert_eq!(splits("echo @"), [true]);
        assert_eq!(splits("echo z=@"), [true]);
        assert_eq!(splits("cmd --opt=@"), [true]);
        assert_eq!(splits("echo @.bai"), [true]);
        assert_eq!(splits("[ @ == a ]"), [true]);
        assert_eq!(splits("if [ -n @ ]; then :; fi"), [true]);
        assert_eq!(splits("@ --flag"), [true]);
        assert_eq!(splits("echo \"a\"@\"b\""), [true]);
        assert_eq!(splits("cat > @"), [true]);
        assert_eq!(splits("cmd 2>&1 @"), [true]);
        assert_eq!(splits("cat <<< @"), [true]);
        assert_eq!(splits("cmd \\\n  @"), [true]);
        assert_eq!(splits("env X=@ cmd"), [true]);
    }

    #[test]
    fn quotes_are_not_split() {
        assert_eq!(splits("echo \"@\""), [false]);
        assert_eq!(splits("echo '@'"), [false]);
        assert_eq!(splits("echo $'@'"), [false]);
        assert_eq!(splits("echo \"it's @\""), [false]);
        assert_eq!(splits("echo 'say \"hi\"' @"), [true]);
        assert_eq!(splits("echo \"a \\\" @\""), [false]);
        assert_eq!(splits("echo \\\" @"), [true]);
        assert_eq!(splits("echo \"${x:-@}\""), [false]);
        assert_eq!(splits("echo ${x:-@}"), [true]);
    }

    #[test]
    fn assignments_are_not_split() {
        assert_eq!(splits("x=@"), [false]);
        assert_eq!(splits("x+=@"), [false]);
        assert_eq!(splits("x[0]=@"), [false]);
        assert_eq!(splits("x=@ y=@ cmd @"), [false, false, true]);
        assert_eq!(splits("export x=@"), [false]);
        assert_eq!(splits("local -r x=@"), [false]);
        assert_eq!(splits("readonly x=@ y"), [false]);
        assert_eq!(splits("echo a; x=@"), [false]);
        assert_eq!(splits("x=$(echo @)"), [true]);
        assert_eq!(splits("x=\"$(echo @)\""), [true]);
        assert_eq!(splits("x=( @ )"), [true]);
        assert_eq!(splits("x+=( a @ ) y=@"), [true, false]);
        assert_eq!(splits("a[\"key\"]=@"), [false]);
        assert_eq!(splits("a[$i]=@"), [false]);
        assert_eq!(splits("2>/dev/null y=@ env @"), [false, true]);
        assert_eq!(splits("echo 2>@"), [true]);
    }

    #[test]
    fn nested_code_is_split() {
        assert_eq!(splits("echo $(echo @)"), [true]);
        assert_eq!(splits("echo \"$(echo @)\""), [true]);
        assert_eq!(splits("echo \"$(echo \"@\")\""), [false]);
        assert_eq!(splits("echo `echo @`"), [true]);
        assert_eq!(splits("diff <(sort @) >(cat @)"), [true, true]);
        assert_eq!(splits("( cd @ )"), [true]);
        assert_eq!(splits("{ echo @; }"), [true]);
    }

    #[test]
    fn special_contexts_are_not_split() {
        assert_eq!(splits("[[ @ == a ]] && echo @"), [false, true]);
        assert_eq!(splits("if [[ -f @ ]]; then echo @; fi"), [false, true]);
        assert_eq!(splits("echo $(( @ + 1 )) @"), [false, true]);
        assert_eq!(splits("(( x = @ )); echo @"), [false, true]);
        assert_eq!(
            splits("case @ in a|@) echo @;; esac; echo @"),
            [false, false, true, true]
        );
        assert_eq!(
            splits("case @ in\n  (a) echo @\n  ;;\n  b) x=@ ;;\nesac\necho @"),
            [false, true, false, true]
        );
        assert_eq!(
            splits("x=$(case @ in a) echo @;; esac) @"),
            [false, true, true]
        );
    }

    #[test]
    fn loop_lists_are_split() {
        assert_eq!(splits("for f in @; do echo @; done"), [true, true]);
        assert_eq!(splits("for f in a @\ndo\n  x=@\ndone"), [true, false]);
        assert_eq!(splits("select f in @; do :; done"), [true]);
        assert_eq!(
            splits("for ((i = @; i < 3; i++)); do echo @; done"),
            [false, true]
        );
    }

    #[test]
    fn comments_are_not_split() {
        assert_eq!(splits("# don't use @\necho @"), [false, true]);
        assert_eq!(splits("echo a # it's @\necho @"), [false, true]);
        assert_eq!(splits("echo a#b @"), [true]);
        assert_eq!(splits("echo $# @"), [true]);
        assert_eq!(splits("echo ${#x} @"), [true]);
    }

    #[test]
    fn heredocs_are_not_split() {
        assert_eq!(splits("cat <<EOF\n@\nEOF\necho @"), [false, true]);
        assert_eq!(
            splits("cat <<'EOF' > @\nit's @\nEOF\necho @"),
            [true, false, true]
        );
        assert_eq!(splits("cat <<-\"EOF\"\n\t@\n\tEOF\necho @"), [false, true]);
        assert_eq!(
            splits("cat <<A <<B\n@\nA\n@\nB\necho @"),
            [false, false, true]
        );
        assert_eq!(splits("cat <<EOF\n@EOF\nEOF\necho @"), [false, true]);
        assert_eq!(splits("x=$(cat <<EOF\n@\nEOF\n)\necho @"), [false, true]);
        assert_eq!(
            splits("cat <<'END MARK'\n@\nEND MARK\necho @"),
            [false, true]
        );
        assert_eq!(splits("cat <<E\\OF\n@\nEOF\necho @"), [false, true]);
        assert_eq!(splits("cat <<EOF\n EOF\n@\nEOF\necho @"), [false, true]);
        assert_eq!(splits("cat <<EOF\n\tEOF\n@\nEOF\necho @"), [false, true]);
        assert_eq!(splits("cat <<-EOF\n@\n\t\tEOF\necho @"), [false, true]);
        assert_eq!(splits("cat <<EOF\r\n@\r\nEOF\r\necho @"), [false, true]);
    }

    #[test]
    fn unterminated_constructs_do_not_panic() {
        assert_eq!(splits("echo \"@"), [false]);
        assert_eq!(splits("echo ) @"), [true]);
        assert_eq!(splits("echo ]] @"), [true]);
        assert_eq!(splits("echo $(@"), [true]);
        assert_eq!(splits("echo \\"), Vec::<bool>::new());
        assert_eq!(splits("cat <<\n@"), [true]);
    }
}
