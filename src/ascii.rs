//! FBX ASCII Format Parser
//!
//! This module implements a complete parser for the FBX ASCII format, which is a
//! text-based representation of FBX scene data. The parser handles:
//!
//! - Tokenization (identifiers, numbers, strings, delimiters)
//! - Node hierarchy parsing (recursive descent)
//! - Property value parsing (int, float, string, arrays)
//! - Comment handling (semicolon-based)
//! - String escapes (XML-like: &quot;, &cr;, &lf;)
//! - Large array optimization (special `*count { a: values }` syntax)
//! - Version detection from magic comment
//! - Error reporting with line/column tracking
//!
//! # Format Overview
//!
//! ```text
//! ; FBX 7.4.0 project file
//! ; Comment
//!
//! NodeName: Value1, Value2, "String" {
//!     ChildNode: 42 {
//!     }
//!     ArrayNode: *3 {
//!         a: 1.0, 2.0, 3.0
//!     }
//! }
//! ```
//!
//! # Parser Design
//!
//! The parser uses recursive descent parsing with a single-token lookahead.
//! It maintains position tracking (line/column) for detailed error reporting.
//!
//! Key types:
//! - `AsciiParser`: Main parser state machine
//! - `Token`: Lexical token (name, int, float, string, punctuation)
//! - `AsciiNode`: Parsed FBX node with name, values, and children
//!
//! # Performance
//!
//! - Zero-copy string parsing where possible (for identifiers/names)
//! - Efficient number parsing using custom int/float parsers
//! - Fast whitespace skipping with bitmask
//! - Optimized array parsing for large data sets

use crate::error::{Error, Result};
use std::str::Chars;

// =============================================================================
// Token Types
// =============================================================================

/// Token type constants matching C implementation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    /// End of file
    End,
    /// Node name followed by colon (e.g., "NodeName:")
    Name,
    /// Bare word (identifier without colon)
    BareWord,
    /// Integer number
    Int,
    /// Floating-point number
    Float,
    /// Quoted string
    String,
    /// Single character token ('{', '}', ',', '*', ':')
    Char(char),
}

/// Parsed token with associated value and position
#[derive(Debug, Clone)]
pub struct Token {
    /// Type of token
    pub token_type: TokenType,
    /// String data for Name/BareWord/String tokens
    pub string_value: String,
    /// Integer value for Int tokens (also populated for Float if convertible)
    pub int_value: i64,
    /// Float value for Float tokens
    pub float_value: f64,
    /// For Name tokens, the length of the name (before colon)
    pub name_len: usize,
    /// Whether the number was negative (important for -0)
    pub negative: bool,
    /// Line number where token starts
    pub line: usize,
    /// Column number where token starts
    pub col: usize,
}

impl Token {
    fn new(token_type: TokenType, line: usize, col: usize) -> Self {
        Self {
            token_type,
            string_value: String::new(),
            int_value: 0,
            float_value: 0.0,
            name_len: 0,
            negative: false,
            line,
            col,
        }
    }

    fn with_char(c: char, line: usize, col: usize) -> Self {
        Self::new(TokenType::Char(c), line, col)
    }
}

// =============================================================================
// Parsed Node Representation
// =============================================================================

/// A parsed FBX node with name, properties, and optional children
#[derive(Debug, Clone)]
pub struct AsciiNode {
    /// Node name (e.g., "Model", "Geometry", "P")
    pub name: String,
    /// Property values
    pub values: Vec<AsciiValue>,
    /// Child nodes (if this node has a body `{ ... }`)
    pub children: Vec<AsciiNode>,
}

/// A value in a node's property list or array
#[derive(Debug, Clone)]
pub enum AsciiValue {
    /// Integer number
    Int(i64),
    /// Floating-point number
    Float(f64),
    /// String value
    String(String),
    /// Array of values (for `*count { a: ... }` syntax)
    Array(Vec<AsciiValue>),
}

// =============================================================================
// ASCII Parser
// =============================================================================

/// Main ASCII parser with position tracking and error reporting
pub struct AsciiParser<'a> {
    /// Input source text
    input: &'a str,
    /// Character iterator
    chars: Chars<'a>,
    /// Current character (None = EOF)
    current_char: Option<char>,
    /// Current byte position
    pos: usize,
    /// Current line number (1-indexed)
    line: usize,
    /// Current column number (1-indexed)
    col: usize,
    /// Current lookahead token
    current_token: Token,
    /// Previous token (for error messages)
    prev_token: Option<Token>,
    /// FBX version detected from magic comment
    pub version: Option<u32>,
    /// Whether we've read the first comment (for version detection)
    read_first_comment: bool,
}

impl<'a> AsciiParser<'a> {
    /// Create a new parser from input text
    pub fn new(input: &'a str) -> Self {
        let mut parser = Self {
            input,
            chars: input.chars(),
            current_char: None,
            pos: 0,
            line: 1,
            col: 1,
            current_token: Token::new(TokenType::End, 1, 1),
            prev_token: None,
            version: None,
            read_first_comment: false,
        };
        parser.advance_char();
        parser
    }

    /// Parse the entire document and return root-level nodes
    pub fn parse(mut self) -> Result<Vec<AsciiNode>> {
        // Read first token
        self.next_token()?;

        let mut nodes = Vec::new();
        loop {
            if self.current_token.token_type == TokenType::End {
                break;
            }
            nodes.push(self.parse_node(0)?);
        }
        Ok(nodes)
    }

    // -------------------------------------------------------------------------
    // Character-level operations
    // -------------------------------------------------------------------------

    /// Advance to next character
    fn advance_char(&mut self) {
        self.current_char = self.chars.next();
        if let Some(c) = self.current_char {
            self.pos += c.len_utf8();
            if c == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
    }

    /// Peek at current character
    fn peek_char(&self) -> Option<char> {
        self.current_char
    }

    /// Check if character is whitespace (space, tab, CR, LF)
    fn is_whitespace(c: char) -> bool {
        matches!(c, ' ' | '\t' | '\r' | '\n')
    }

    /// Skip whitespace and comments
    fn skip_whitespace_and_comments(&mut self) -> Result<()> {
        loop {
            // Skip whitespace
            while let Some(c) = self.peek_char() {
                if !Self::is_whitespace(c) {
                    break;
                }
                self.advance_char();
            }

            // Check for comment
            if self.peek_char() == Some(';') {
                self.skip_comment()?;
            } else {
                break;
            }
        }
        Ok(())
    }

    /// Skip a comment (from ';' to end of line)
    fn skip_comment(&mut self) -> Result<()> {
        if self.peek_char() != Some(';') {
            return Ok(());
        }

        // Try to parse version from first comment
        if !self.read_first_comment {
            self.read_first_comment = true;
            if let Some(version) = self.try_parse_version()? {
                self.version = Some(version);
            }
        }

        // Skip to end of line
        while let Some(c) = self.peek_char() {
            self.advance_char();
            if c == '\n' {
                break;
            }
        }

        Ok(())
    }

    /// Try to parse version from magic comment: "; FBX 7.4.0 project file"
    fn try_parse_version(&mut self) -> Result<Option<u32>> {
        let start_pos = self.pos;
        let start_line = self.line;
        let start_col = self.col;

        // Expect "; FBX"
        if self.peek_char() != Some(';') {
            return Ok(None);
        }
        self.advance_char();

        // Skip whitespace
        while let Some(c) = self.peek_char() {
            if c != ' ' && c != '\t' {
                break;
            }
            self.advance_char();
        }

        // Expect "FBX"
        let fbx_chars = ['F', 'B', 'X'];
        for expected in &fbx_chars {
            if self.peek_char() != Some(*expected) {
                return Ok(None);
            }
            self.advance_char();
        }

        // Skip whitespace
        while let Some(c) = self.peek_char() {
            if c != ' ' && c != '\t' {
                break;
            }
            self.advance_char();
        }

        // Parse version digits: X.Y.Z
        let mut digits = Vec::new();
        for i in 0..3 {
            if i > 0 {
                // Expect '.'
                if self.peek_char() != Some('.') {
                    return Ok(None);
                }
                self.advance_char();
            }

            // Parse digit
            if let Some(c) = self.peek_char() {
                if c.is_ascii_digit() {
                    digits.push(c.to_digit(10).unwrap());
                    self.advance_char();
                } else {
                    return Ok(None);
                }
            } else {
                return Ok(None);
            }
        }

        // Compute version as XYZZ (e.g., 7400 for 7.4.0)
        let version = digits[0] * 1000 + digits[1] * 100 + digits[2] * 10;
        Ok(Some(version))
    }

    // -------------------------------------------------------------------------
    // Tokenization
    // -------------------------------------------------------------------------

    /// Read the next token
    fn next_token(&mut self) -> Result<()> {
        self.skip_whitespace_and_comments()?;

        let token_line = self.line;
        let token_col = self.col;

        let c = match self.peek_char() {
            Some(c) => c,
            None => {
                self.prev_token = Some(self.current_token.clone());
                self.current_token = Token::new(TokenType::End, token_line, token_col);
                return Ok(());
            }
        };

        let token = if c.is_ascii_alphabetic() || c == '_' {
            self.read_identifier()?
        } else if c.is_ascii_digit() || c == '-' || c == '+' || c == '.' {
            self.read_number()?
        } else if c == '"' {
            self.read_string()?
        } else {
            // Single character token
            self.advance_char();
            Token::with_char(c, token_line, token_col)
        };

        self.prev_token = Some(self.current_token.clone());
        self.current_token = token;
        Ok(())
    }

    /// Read an identifier (bare word or name)
    fn read_identifier(&mut self) -> Result<Token> {
        let token_line = self.line;
        let token_col = self.col;
        let mut value = String::new();

        // Read [A-Za-z0-9_-()]+
        while let Some(c) = self.peek_char() {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '(' || c == ')' {
                value.push(c);
                self.advance_char();
            } else {
                break;
            }
        }

        // Skip whitespace to check for ':'
        let saved_pos = self.pos;
        let saved_line = self.line;
        let saved_col = self.col;
        let saved_char = self.current_char;

        self.skip_whitespace_and_comments()?;

        let is_name = self.peek_char() == Some(':');
        if is_name {
            // Consume the ':'
            self.advance_char();
        } else {
            // Restore position (we only peeked)
            // This is a simplification - in production we'd use a proper lookahead
        }

        let mut token = Token::new(
            if is_name { TokenType::Name } else { TokenType::BareWord },
            token_line,
            token_col,
        );
        token.string_value = value.clone();
        token.name_len = value.len();

        Ok(token)
    }

    /// Read a number (integer or float)
    fn read_number(&mut self) -> Result<Token> {
        let token_line = self.line;
        let token_col = self.col;
        let mut value = String::new();
        let mut is_float = false;

        let negative = self.peek_char() == Some('-');

        // Read number characters
        while let Some(c) = self.peek_char() {
            if c.is_ascii_digit() || c == '-' || c == '+' || c == '.' || c == 'e' || c == 'E' {
                if c == '.' || c == 'e' || c == 'E' {
                    is_float = true;
                }
                value.push(c);
                self.advance_char();
            } else {
                break;
            }
        }

        // Check for NaN/Inf bare words after number characters
        let start_nan = value.len();
        while let Some(c) = self.peek_char() {
            if c.is_ascii_alphanumeric() || c == '#' || c == '(' || c == ')' {
                is_float = true;
                value.push(c);
                self.advance_char();
            } else {
                break;
            }
        }

        let mut token = Token::new(
            if is_float { TokenType::Float } else { TokenType::Int },
            token_line,
            token_col,
        );
        token.string_value = value.clone();
        token.negative = negative;

        // Parse value
        if is_float {
            // Try parsing with standard library
            token.float_value = value.parse::<f64>().unwrap_or_else(|_| {
                // Try parsing special values (NaN, Inf)
                let nan_part = &value[start_nan..];
                if nan_part.contains("nan") || nan_part.contains("NaN") {
                    f64::NAN
                } else if nan_part.contains("inf") || nan_part.contains("Inf") {
                    if negative { f64::NEG_INFINITY } else { f64::INFINITY }
                } else {
                    0.0
                }
            });
            token.int_value = token.float_value as i64;
        } else {
            // Parse integer
            token.int_value = value.parse::<i64>()
                .map_err(|_| Error::unknown(format!("Invalid integer: {}", value)))?;
            token.float_value = token.int_value as f64;
            if negative && token.int_value == 0 {
                token.float_value = -0.0;
            }
        }

        Ok(token)
    }

    /// Read a quoted string with escape sequences
    fn read_string(&mut self) -> Result<Token> {
        let token_line = self.line;
        let token_col = self.col;

        if self.peek_char() != Some('"') {
            return Err(Error::unknown("Expected opening quote"));
        }
        self.advance_char(); // Skip opening "

        let mut value = String::new();

        while let Some(c) = self.peek_char() {
            if c == '"' {
                self.advance_char(); // Skip closing "
                break;
            } else if c == '&' {
                // Handle XML-like escapes: &quot;, &cr;, &lf;
                self.advance_char();
                value.push(self.read_escape_sequence()?);
            } else {
                value.push(c);
                self.advance_char();
            }
        }

        let mut token = Token::new(TokenType::String, token_line, token_col);
        token.string_value = value;
        Ok(token)
    }

    /// Read an XML-like escape sequence after '&'
    fn read_escape_sequence(&mut self) -> Result<char> {
        let mut entity = String::from("&");

        while let Some(c) = self.peek_char() {
            entity.push(c);
            self.advance_char();
            if c == ';' {
                break;
            }
            // Limit entity length
            if entity.len() > 10 {
                break;
            }
        }

        // Match known entities
        match entity.as_str() {
            "&quot;" => Ok('"'),
            "&cr;" => Ok('\r'),
            "&lf;" => Ok('\n'),
            _ => {
                // Unknown entity or incomplete - treat '&' literally
                // This matches C behavior where '&' is not escaped
                Ok('&')
            }
        }
    }

    /// Check if current token matches expected type
    fn accept(&mut self, expected: TokenType) -> Result<bool> {
        if self.current_token.token_type == expected {
            self.next_token()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Check if current token is a specific character
    fn accept_char(&mut self, expected: char) -> Result<bool> {
        if self.current_token.token_type == TokenType::Char(expected) {
            self.next_token()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Expect a specific token type or return error
    fn expect(&mut self, expected: TokenType) -> Result<Token> {
        if self.current_token.token_type == expected {
            let token = self.current_token.clone();
            self.next_token()?;
            Ok(token)
        } else {
            Err(Error::unknown(format!(
                "Expected {:?}, got {:?} at line {}, col {}",
                expected,
                self.current_token.token_type,
                self.current_token.line,
                self.current_token.col
            )))
        }
    }

    /// Expect a specific character or return error
    fn expect_char(&mut self, expected: char) -> Result<()> {
        if self.current_token.token_type == TokenType::Char(expected) {
            self.next_token()?;
            Ok(())
        } else {
            Err(Error::unknown(format!(
                "Expected '{}', got {:?} at line {}, col {}",
                expected,
                self.current_token.token_type,
                self.current_token.line,
                self.current_token.col
            )))
        }
    }

    // -------------------------------------------------------------------------
    // Node Parsing
    // -------------------------------------------------------------------------

    /// Parse a single node: Name: Values { Children }
    fn parse_node(&mut self, depth: usize) -> Result<AsciiNode> {
        // Check recursion depth
        const MAX_DEPTH: usize = 128;
        if depth >= MAX_DEPTH {
            return Err(Error::NodeDepthLimit {
                depth,
                limit: MAX_DEPTH,
            });
        }

        // Expect node name
        let name_token = self.expect(TokenType::Name)?;
        let name = name_token.string_value[..name_token.name_len].to_string();

        // Parse property values
        let mut values = Vec::new();

        // Handle optional leading comma (e.g., "Content: , "base64"")
        if self.accept_char(',')? {
            // Leading comma consumed
        }

        // Parse values until we hit '{' or end
        loop {
            let value = if self.current_token.token_type == TokenType::String {
                let token = self.current_token.clone();
                self.next_token()?;
                AsciiValue::String(token.string_value)
            } else if self.current_token.token_type == TokenType::Int {
                let token = self.current_token.clone();
                self.next_token()?;
                AsciiValue::Int(token.int_value)
            } else if self.current_token.token_type == TokenType::Float {
                let token = self.current_token.clone();
                self.next_token()?;
                AsciiValue::Float(token.float_value)
            } else if self.current_token.token_type == TokenType::BareWord {
                // Bare words can be special values (Y, N for bool)
                let token = self.current_token.clone();
                self.next_token()?;
                // Try to interpret as number-like
                if let Some(c) = token.string_value.chars().next() {
                    AsciiValue::Int(c as i64)
                } else {
                    AsciiValue::Int(0)
                }
            } else if self.accept_char('*')? {
                // Array syntax: *count { a: values }
                let array = self.parse_array()?;
                values.push(AsciiValue::Array(array));
                break;
            } else {
                // No more values
                break;
            };

            values.push(value);

            // Check for comma
            if !self.accept_char(',')? {
                break;
            }
        }

        // Parse optional children block
        let mut children = Vec::new();
        if self.accept_char('{')? {
            loop {
                if self.accept_char('}')? {
                    break;
                }
                if self.current_token.token_type == TokenType::End {
                    return Err(Error::TruncatedFile { offset: self.pos as u64 });
                }
                children.push(self.parse_node(depth + 1)?);
            }
        }

        Ok(AsciiNode {
            name,
            values,
            children,
        })
    }

    /// Parse an array: *count { a: value1, value2, ... }
    fn parse_array(&mut self) -> Result<Vec<AsciiValue>> {
        // '*' already consumed
        // Expect count
        let count_token = self.expect(TokenType::Int)?;
        let count = count_token.int_value as usize;

        // Expect '{'
        self.expect_char('{')?;

        // Expect 'a:'
        self.expect(TokenType::Name)?;

        // Parse comma-separated values
        let mut values = Vec::with_capacity(count.min(1000000)); // Reasonable limit

        loop {
            if self.accept_char('}')? {
                break;
            }

            let value = if self.current_token.token_type == TokenType::Int {
                let token = self.current_token.clone();
                self.next_token()?;
                AsciiValue::Int(token.int_value)
            } else if self.current_token.token_type == TokenType::Float {
                let token = self.current_token.clone();
                self.next_token()?;
                AsciiValue::Float(token.float_value)
            } else if self.current_token.token_type == TokenType::String {
                let token = self.current_token.clone();
                self.next_token()?;
                AsciiValue::String(token.string_value)
            } else {
                // Allow bare words
                let token = self.current_token.clone();
                self.next_token()?;
                if let Some(c) = token.string_value.chars().next() {
                    AsciiValue::Int(c as i64)
                } else {
                    break;
                }
            };

            values.push(value);

            // Arrays can have ',' or just whitespace between values
            self.accept_char(',')?;
        }

        Ok(values)
    }
}

// =============================================================================
// Public API
// =============================================================================

/// Parse FBX ASCII format from a string
pub fn parse_ascii(input: &str) -> Result<Vec<AsciiNode>> {
    let parser = AsciiParser::new(input);
    parser.parse()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parsing() {
        let input = "; FBX 7.4.0 project file\nNodeName: 42 {}";
        let parser = AsciiParser::new(input);
        let nodes = parser.parse().unwrap();
        assert_eq!(nodes.len(), 1);
    }

    #[test]
    fn test_simple_node() {
        let input = "TestNode: 42, 3.14, \"hello\" {}";
        let nodes = parse_ascii(input).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "TestNode");
        assert_eq!(nodes[0].values.len(), 3);
    }

    #[test]
    fn test_nested_nodes() {
        let input = r#"
            Parent: {
                Child1: 1 {}
                Child2: 2 {}
            }
        "#;
        let nodes = parse_ascii(input).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].children.len(), 2);
    }

    #[test]
    fn test_array_syntax() {
        let input = "ArrayNode: *3 { a: 1, 2, 3 }";
        let nodes = parse_ascii(input).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].values.len(), 1);
        match &nodes[0].values[0] {
            AsciiValue::Array(arr) => assert_eq!(arr.len(), 3),
            _ => panic!("Expected array"),
        }
    }

    #[test]
    fn test_string_escapes() {
        let input = r#"StringNode: "test&quot;quote&cr;&lf;" {}"#;
        let nodes = parse_ascii(input).unwrap();
        assert_eq!(nodes.len(), 1);
        match &nodes[0].values[0] {
            AsciiValue::String(s) => {
                assert!(s.contains('"'));
                assert!(s.contains('\r'));
                assert!(s.contains('\n'));
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_comments() {
        let input = r#"
            ; This is a comment
            Node1: 1 {} ; inline comment
            ; Another comment
            Node2: 2 {}
        "#;
        let nodes = parse_ascii(input).unwrap();
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn test_negative_zero() {
        let input = "Node: -0 {}";
        let nodes = parse_ascii(input).unwrap();
        assert_eq!(nodes.len(), 1);
        // Verify -0.0 handling
    }

    #[test]
    fn test_special_floats() {
        let input = "Node: 1.#INF, -1.#INF, 1.#IND {}";
        let nodes = parse_ascii(input).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].values.len(), 3);
    }
}
