//! ASCII FBX File Writer
//!
//! This module encodes FbxDocument structures into ASCII FBX format.
//!
//! # ASCII Format
//!
//! ```text
//! ; FBX 7.4.0 project file
//! ; Created by ufbx-rust
//!
//! FBXHeaderExtension:  {
//!     FBXHeaderVersion: 1003
//!     FBXVersion: 7400
//! }
//!
//! Objects:  {
//!     Geometry: 123456789, "Geometry::Cube", "Mesh" {
//!         Vertices: *24 {
//!             a: -1.0,1.0,-1.0,...
//!         }
//!     }
//! }
//! ```

use crate::binary::{FbxDocument, FbxNode, Value, ValueArray, ArrayData};
use crate::error::{Error, Result};
use crate::writer::SaveOpts;
use std::io::Write;

// =============================================================================
// ASCII Writer
// =============================================================================

/// Writes FbxDocument to ASCII FBX format
pub struct AsciiWriter<W: Write> {
    writer: W,
    indent_level: usize,
    indent_string: String,
    version: u32,
}

impl<W: Write> AsciiWriter<W> {
    pub fn new(writer: W, version: u32, indent: String) -> Self {
        Self {
            writer,
            indent_level: 0,
            indent_string: indent,
            version,
        }
    }

    /// Write the complete FBX document
    pub fn write_document(&mut self, doc: &FbxDocument) -> Result<()> {
        // Write comment header
        self.write_comment(&format!("FBX {}.{}.0 project file", self.version / 1000, (self.version % 1000) / 100))?;
        self.write_comment("Created by ufbx-rust")?;
        self.write_newline()?;

        // Write all top-level nodes
        for node in &doc.nodes {
            self.write_node(node)?;
            self.write_newline()?;
        }

        Ok(())
    }

    /// Write a comment line
    fn write_comment(&mut self, comment: &str) -> Result<()> {
        write!(self.writer, "; {}\n", comment).map_err(|e| Error::Io { message: e.to_string() })
    }

    /// Write a newline
    fn write_newline(&mut self) -> Result<()> {
        writeln!(self.writer).map_err(|e| Error::Io { message: e.to_string() })
    }

    /// Write current indentation
    fn write_indent(&mut self) -> Result<()> {
        for _ in 0..self.indent_level {
            write!(self.writer, "{}", self.indent_string).map_err(|e| Error::Io { message: e.to_string() })?;
        }
        Ok(())
    }

    /// Write a node
    fn write_node(&mut self, node: &FbxNode) -> Result<()> {
        self.write_indent()?;

        // Write node name
        write!(self.writer, "{}", node.name).map_err(|e| Error::Io { message: e.to_string() })?;

        // Write node properties
        if !node.values.is_empty() {
            write!(self.writer, ":").map_err(|e| Error::Io { message: e.to_string() })?;
            for (i, value) in node.values.iter().enumerate() {
                if i > 0 {
                    write!(self.writer, ",").map_err(|e| Error::Io { message: e.to_string() })?;
                }
                write!(self.writer, " ").map_err(|e| Error::Io { message: e.to_string() })?;
                self.write_value(value)?;
            }
        }

        // Check if node has children or arrays
        let has_content = !node.children.is_empty() || node.array.is_some();

        if has_content {
            writeln!(self.writer, " {{").map_err(|e| Error::Io { message: e.to_string() })?;
            self.indent_level += 1;

            // Write array if present
            if let Some(ref array) = node.array {
                self.write_array(array)?;
            }

            // Write children
            for child in &node.children {
                self.write_node(child)?;
            }

            self.indent_level -= 1;
            self.write_indent()?;
            writeln!(self.writer, "}}").map_err(|e| Error::Io { message: e.to_string() })?;
        } else {
            writeln!(self.writer).map_err(|e| Error::Io { message: e.to_string() })?;
        }

        Ok(())
    }

    /// Write a property value
    fn write_value(&mut self, value: &Value) -> Result<()> {
        match value {
            Value::Bool(v) => write!(self.writer, "{}", if *v { 1 } else { 0 }),
            Value::Int16(v) => write!(self.writer, "{}", v),
            Value::Int32(v) => write!(self.writer, "{}", v),
            Value::Int64(v) => write!(self.writer, "{}", v),
            Value::Float32(v) => write!(self.writer, "{}", v),
            Value::Float64(v) => write!(self.writer, "{}", v),
            Value::String(s) => {
                // Escape quotes in strings
                let escaped = s.as_str().replace("\"", "\\\"");
                write!(self.writer, "\"{}\"", escaped)
            }
            Value::Number { i, .. } => write!(self.writer, "{}", i),
        }.map_err(|e| Error::Io { message: e.to_string() })
    }

    /// Write an array property
    fn write_array(&mut self, array: &ValueArray) -> Result<()> {
        self.write_indent()?;

        let len = array.data.len();
        write!(self.writer, "a: ").map_err(|e| Error::Io { message: e.to_string() })?;

        match &array.data {
            ArrayData::Bool(data) => {
                for (i, &v) in data.iter().enumerate() {
                    if i > 0 { write!(self.writer, ",").map_err(|e| Error::Io { message: e.to_string() })?; }
                    write!(self.writer, "{}", if v { 1 } else { 0 }).map_err(|e| Error::Io { message: e.to_string() })?;
                }
            }
            ArrayData::I32(data) => {
                for (i, &v) in data.iter().enumerate() {
                    if i > 0 { write!(self.writer, ",").map_err(|e| Error::Io { message: e.to_string() })?; }
                    write!(self.writer, "{}", v).map_err(|e| Error::Io { message: e.to_string() })?;
                }
            }
            ArrayData::I64(data) => {
                for (i, &v) in data.iter().enumerate() {
                    if i > 0 { write!(self.writer, ",").map_err(|e| Error::Io { message: e.to_string() })?; }
                    write!(self.writer, "{}", v).map_err(|e| Error::Io { message: e.to_string() })?;
                }
            }
            ArrayData::F32(data) => {
                for (i, &v) in data.iter().enumerate() {
                    if i > 0 { write!(self.writer, ",").map_err(|e| Error::Io { message: e.to_string() })?; }
                    write!(self.writer, "{}", v).map_err(|e| Error::Io { message: e.to_string() })?;
                }
            }
            ArrayData::F64(data) => {
                for (i, &v) in data.iter().enumerate() {
                    if i > 0 { write!(self.writer, ",").map_err(|e| Error::Io { message: e.to_string() })?; }
                    write!(self.writer, "{}", v).map_err(|e| Error::Io { message: e.to_string() })?;
                }
            }
            ArrayData::U8(data) => {
                for (i, &v) in data.iter().enumerate() {
                    if i > 0 { write!(self.writer, ",").map_err(|e| Error::Io { message: e.to_string() })?; }
                    write!(self.writer, "{}", v).map_err(|e| Error::Io { message: e.to_string() })?;
                }
            }
            ArrayData::String(data) => {
                for (i, s) in data.iter().enumerate() {
                    if i > 0 { write!(self.writer, ",").map_err(|e| Error::Io { message: e.to_string() })?; }
                    let escaped = s.as_str().replace("\"", "\\\"");
                    write!(self.writer, "\"{}\"", escaped).map_err(|e| Error::Io { message: e.to_string() })?;
                }
            }
        }

        writeln!(self.writer).map_err(|e| Error::Io { message: e.to_string() })?;
        Ok(())
    }
}

// =============================================================================
// Public API
// =============================================================================

/// Write an FbxDocument to ASCII format
pub fn write_ascii(doc: &FbxDocument, opts: &SaveOpts) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut writer = AsciiWriter::new(&mut buf, opts.version, opts.ascii_indent.clone());
    writer.write_document(doc)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binary::FbxNode;

    #[test]
    fn test_write_comment() {
        let mut buf = Vec::new();
        let mut writer = AsciiWriter::new(&mut buf, 7400, "  ".to_string());
        writer.write_comment("Test comment").unwrap();

        let output = String::from_utf8(buf).unwrap();
        assert_eq!(output, "; Test comment\n");
    }

    #[test]
    fn test_write_simple_node() {
        let mut buf = Vec::new();
        let mut writer = AsciiWriter::new(&mut buf, 7400, "  ".to_string());

        let mut node = FbxNode::new("TestNode");
        node.values.push(Value::Int32(42));
        node.values.push(Value::String("hello".into()));

        writer.write_node(&node).unwrap();

        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("TestNode:"));
        assert!(output.contains("42"));
        assert!(output.contains("\"hello\""));
    }

    #[test]
    fn test_write_nested_nodes() {
        let mut buf = Vec::new();
        let mut writer = AsciiWriter::new(&mut buf, 7400, "    ".to_string());

        let mut root = FbxNode::new("Root");
        let mut child = FbxNode::new("Child");
        child.values.push(Value::Int32(123));
        root.children.push(child);

        writer.write_node(&root).unwrap();

        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("Root {"));
        assert!(output.contains("Child:"));
        assert!(output.contains("}"));
    }
}
