//! Binary FBX File Writer
//!
//! This module encodes FbxDocument structures into binary FBX format.
//!
//! # Binary Format
//!
//! ```text
//! [Header: 27 bytes]
//!   - Magic: "Kaydara FBX Binary  \x00\x1a" (22 bytes)
//!   - Endian: 0x00 (1 byte, little-endian)
//!   - Version: u32 LE (4 bytes)
//!
//! [Node Tree]
//!   - Recursive nodes
//!   - NULL terminator (13 or 25 zero bytes)
//!
//! [Footer]
//!   - Padding and version marker
//! ```

use crate::binary::{FbxDocument, FbxNode, Value, ValueArray, ArrayData};
use crate::error::{Error, Result};
use crate::writer::SaveOpts;
use byteorder::{LittleEndian, WriteBytesExt};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;

// =============================================================================
// Constants
// =============================================================================

const BINARY_MAGIC: &[u8; 22] = b"Kaydara FBX Binary  \x00\x1a";
const FBX_VERSION_7500: u32 = 7500;
const ARRAY_COMPRESS_THRESHOLD: usize = 128;

// =============================================================================
// Binary Writer
// =============================================================================

/// Writes FbxDocument to binary FBX format
pub struct BinaryWriter<W: Write> {
    writer: W,
    version: u32,
    compress_arrays: bool,
}

impl<W: Write> BinaryWriter<W> {
    pub fn new(writer: W, version: u32, compress_arrays: bool) -> Self {
        Self {
            writer,
            version,
            compress_arrays,
        }
    }

    /// Write the complete FBX document
    pub fn write_document(&mut self, doc: &FbxDocument) -> Result<()> {
        // Write header
        self.write_header()?;

        // Write all top-level nodes
        for node in &doc.nodes {
            self.write_node(node, 0)?;
        }

        // Write NULL terminator
        self.write_null_node()?;

        // Write footer
        self.write_footer()?;

        Ok(())
    }

    /// Write binary header
    fn write_header(&mut self) -> Result<()> {
        // Magic bytes
        self.writer.write_all(BINARY_MAGIC)
            .map_err(|e| Error::Io { message: e.to_string() })?;

        // Endian flag (always little-endian)
        self.writer.write_u8(0)
            .map_err(|e| Error::Io { message: e.to_string() })?;

        // Version
        self.writer.write_u32::<LittleEndian>(self.version)
            .map_err(|e| Error::Io { message: e.to_string() })?;

        Ok(())
    }

    /// Write a node (version-dependent format)
    fn write_node(&mut self, node: &FbxNode, depth: u32) -> Result<()> {
        if self.version >= FBX_VERSION_7500 {
            self.write_node_64bit(node, depth)
        } else {
            self.write_node_32bit(node, depth)
        }
    }

    /// Write node in 32-bit format (FBX < 7500)
    fn write_node_32bit(&mut self, node: &FbxNode, depth: u32) -> Result<()> {
        // Calculate sizes
        let mut property_data = Vec::new();
        for value in &node.values {
            self.write_property_to_buffer(&mut property_data, value)?;
        }
        if let Some(ref array) = node.array {
            self.write_array_to_buffer(&mut property_data, array)?;
        }

        // Calculate children data
        let mut children_data = Vec::new();
        for child in &node.children {
            let mut child_writer = BinaryWriter::new(&mut children_data, self.version, self.compress_arrays);
            child_writer.write_node(child, depth + 1)?;
        }

        // Add NULL terminator for children
        if !node.children.is_empty() {
            for _ in 0..13 {
                children_data.write_u8(0).map_err(|e| Error::Io { message: e.to_string() })?;
            }
        }

        // Calculate end offset
        let header_size = 13;
        let name_size = node.name.len();
        let end_offset = header_size + name_size + property_data.len() + children_data.len();

        // Write node header
        self.writer.write_u32::<LittleEndian>(end_offset as u32)
            .map_err(|e| Error::Io { message: e.to_string() })?;
        self.writer.write_u32::<LittleEndian>((node.values.len() + if node.array.is_some() { 1 } else { 0 }) as u32)
            .map_err(|e| Error::Io { message: e.to_string() })?;
        self.writer.write_u32::<LittleEndian>(property_data.len() as u32)
            .map_err(|e| Error::Io { message: e.to_string() })?;
        self.writer.write_u8(node.name.len() as u8)
            .map_err(|e| Error::Io { message: e.to_string() })?;

        // Write node name
        self.writer.write_all(node.name.as_bytes())
            .map_err(|e| Error::Io { message: e.to_string() })?;

        // Write properties
        self.writer.write_all(&property_data)
            .map_err(|e| Error::Io { message: e.to_string() })?;

        // Write children
        self.writer.write_all(&children_data)
            .map_err(|e| Error::Io { message: e.to_string() })?;

        Ok(())
    }

    /// Write node in 64-bit format (FBX >= 7500)
    fn write_node_64bit(&mut self, node: &FbxNode, depth: u32) -> Result<()> {
        // Calculate sizes
        let mut property_data = Vec::new();
        for value in &node.values {
            self.write_property_to_buffer(&mut property_data, value)?;
        }
        if let Some(ref array) = node.array {
            self.write_array_to_buffer(&mut property_data, array)?;
        }

        // Calculate children data
        let mut children_data = Vec::new();
        for child in &node.children {
            let mut child_writer = BinaryWriter::new(&mut children_data, self.version, self.compress_arrays);
            child_writer.write_node(child, depth + 1)?;
        }

        // Add NULL terminator for children
        if !node.children.is_empty() {
            for _ in 0..25 {
                children_data.write_u8(0).map_err(|e| Error::Io { message: e.to_string() })?;
            }
        }

        // Calculate end offset
        let header_size = 25;
        let name_size = node.name.len();
        let end_offset = header_size + name_size + property_data.len() + children_data.len();

        // Write node header
        self.writer.write_u64::<LittleEndian>(end_offset as u64)
            .map_err(|e| Error::Io { message: e.to_string() })?;
        self.writer.write_u64::<LittleEndian>((node.values.len() + if node.array.is_some() { 1 } else { 0 }) as u64)
            .map_err(|e| Error::Io { message: e.to_string() })?;
        self.writer.write_u64::<LittleEndian>(property_data.len() as u64)
            .map_err(|e| Error::Io { message: e.to_string() })?;
        self.writer.write_u8(node.name.len() as u8)
            .map_err(|e| Error::Io { message: e.to_string() })?;

        // Write node name
        self.writer.write_all(node.name.as_bytes())
            .map_err(|e| Error::Io { message: e.to_string() })?;

        // Write properties
        self.writer.write_all(&property_data)
            .map_err(|e| Error::Io { message: e.to_string() })?;

        // Write children
        self.writer.write_all(&children_data)
            .map_err(|e| Error::Io { message: e.to_string() })?;

        Ok(())
    }

    /// Write a property value to a buffer
    fn write_property_to_buffer(&self, buf: &mut Vec<u8>, value: &Value) -> Result<()> {
        match value {
            Value::Bool(v) => {
                buf.write_u8(b'C').map_err(|e| Error::Io { message: e.to_string() })?;
                buf.write_u8(*v as u8).map_err(|e| Error::Io { message: e.to_string() })?;
            }
            Value::Int16(v) => {
                buf.write_u8(b'Y').map_err(|e| Error::Io { message: e.to_string() })?;
                buf.write_i16::<LittleEndian>(*v).map_err(|e| Error::Io { message: e.to_string() })?;
            }
            Value::Int32(v) => {
                buf.write_u8(b'I').map_err(|e| Error::Io { message: e.to_string() })?;
                buf.write_i32::<LittleEndian>(*v).map_err(|e| Error::Io { message: e.to_string() })?;
            }
            Value::Int64(v) => {
                buf.write_u8(b'L').map_err(|e| Error::Io { message: e.to_string() })?;
                buf.write_i64::<LittleEndian>(*v).map_err(|e| Error::Io { message: e.to_string() })?;
            }
            Value::Float32(v) => {
                buf.write_u8(b'F').map_err(|e| Error::Io { message: e.to_string() })?;
                buf.write_f32::<LittleEndian>(*v).map_err(|e| Error::Io { message: e.to_string() })?;
            }
            Value::Float64(v) => {
                buf.write_u8(b'D').map_err(|e| Error::Io { message: e.to_string() })?;
                buf.write_f64::<LittleEndian>(*v).map_err(|e| Error::Io { message: e.to_string() })?;
            }
            Value::String(s) => {
                buf.write_u8(b'S').map_err(|e| Error::Io { message: e.to_string() })?;
                buf.write_u32::<LittleEndian>(s.as_str().len() as u32).map_err(|e| Error::Io { message: e.to_string() })?;
                buf.write_all(s.as_str().as_bytes()).map_err(|e| Error::Io { message: e.to_string() })?;
            }
            Value::Number { i, .. } => {
                // Fallback for old Number type - write as Int64
                buf.write_u8(b'L').map_err(|e| Error::Io { message: e.to_string() })?;
                buf.write_i64::<LittleEndian>(*i).map_err(|e| Error::Io { message: e.to_string() })?;
            }
        }
        Ok(())
    }

    /// Write an array property to a buffer
    fn write_array_to_buffer(&self, buf: &mut Vec<u8>, array: &ValueArray) -> Result<()> {
        match &array.data {
            ArrayData::Bool(data) => self.write_typed_array(buf, b'b', data)?,
            ArrayData::I32(data) => self.write_typed_array(buf, b'i', data)?,
            ArrayData::I64(data) => self.write_typed_array(buf, b'l', data)?,
            ArrayData::F32(data) => self.write_typed_array(buf, b'f', data)?,
            ArrayData::F64(data) => self.write_typed_array(buf, b'd', data)?,
            ArrayData::U8(data) => self.write_typed_array(buf, b'c', data)?,
            ArrayData::String(_) => {
                return Err(Error::UnsupportedVersion { version: 0 }); // String arrays not supported yet
            }
        }
        Ok(())
    }

    /// Write a typed array with optional compression
    fn write_typed_array<T>(&self, buf: &mut Vec<u8>, type_code: u8, data: &[T]) -> Result<()>
    where
        T: Copy,
    {
        let element_size = std::mem::size_of::<T>();
        let raw_bytes = unsafe {
            std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * element_size)
        };

        let should_compress = self.compress_arrays && data.len() > ARRAY_COMPRESS_THRESHOLD;

        buf.write_u8(type_code).map_err(|e| Error::Io { message: e.to_string() })?;
        buf.write_u32::<LittleEndian>(data.len() as u32).map_err(|e| Error::Io { message: e.to_string() })?;

        if should_compress {
            // Compress with DEFLATE
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(raw_bytes).map_err(|e| Error::Io { message: e.to_string() })?;
            let compressed = encoder.finish().map_err(|e| Error::Io { message: e.to_string() })?;

            buf.write_u32::<LittleEndian>(1).map_err(|e| Error::Io { message: e.to_string() })?; // encoding: 1 = DEFLATE
            buf.write_u32::<LittleEndian>(compressed.len() as u32).map_err(|e| Error::Io { message: e.to_string() })?;
            buf.write_all(&compressed).map_err(|e| Error::Io { message: e.to_string() })?;
        } else {
            // Write uncompressed
            buf.write_u32::<LittleEndian>(0).map_err(|e| Error::Io { message: e.to_string() })?; // encoding: 0 = raw
            buf.write_u32::<LittleEndian>(raw_bytes.len() as u32).map_err(|e| Error::Io { message: e.to_string() })?;
            buf.write_all(raw_bytes).map_err(|e| Error::Io { message: e.to_string() })?;
        }

        Ok(())
    }

    /// Write NULL node terminator
    fn write_null_node(&mut self) -> Result<()> {
        let null_size = if self.version >= FBX_VERSION_7500 { 25 } else { 13 };
        for _ in 0..null_size {
            self.writer.write_u8(0).map_err(|e| Error::Io { message: e.to_string() })?;
        }
        Ok(())
    }

    /// Write footer
    fn write_footer(&mut self) -> Result<()> {
        // Unknown padding (120 bytes)
        for _ in 0..120 {
            self.writer.write_u8(0).map_err(|e| Error::Io { message: e.to_string() })?;
        }

        // Footer marker (16 bytes) - varies by version
        let footer_marker: [u8; 16] = if self.version >= 7500 {
            [0xF8, 0x5A, 0x8C, 0x6A, 0xDE, 0xF5, 0xD9, 0x7E,
             0xEC, 0xE9, 0x0C, 0xE3, 0x75, 0x8F, 0x29, 0x0B]
        } else {
            [0xFA, 0xBC, 0xAB, 0x09, 0xD0, 0xC8, 0xD4, 0x66,
             0xB1, 0x76, 0xFB, 0x83, 0x1C, 0xF7, 0x26, 0x7E]
        };

        self.writer.write_all(&footer_marker).map_err(|e| Error::Io { message: e.to_string() })?;

        // Padding to 16-byte alignment
        for _ in 0..4 {
            self.writer.write_u8(0).map_err(|e| Error::Io { message: e.to_string() })?;
        }

        // Version again
        self.writer.write_u32::<LittleEndian>(self.version).map_err(|e| Error::Io { message: e.to_string() })?;

        // Final padding (100 bytes)
        for _ in 0..100 {
            self.writer.write_u8(0).map_err(|e| Error::Io { message: e.to_string() })?;
        }

        // File extension marker
        self.writer.write_all(b"\xF8\x5A\x8C\x6A\xDE\xF5\xD9\x7E")
            .map_err(|e| Error::Io { message: e.to_string() })?;

        Ok(())
    }
}

// =============================================================================
// Public API
// =============================================================================

/// Write an FbxDocument to binary format
pub fn write_binary(doc: &FbxDocument, opts: &SaveOpts) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut writer = BinaryWriter::new(&mut buf, opts.version, opts.compress_arrays);
    writer.write_document(doc)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binary::FbxNode;

    #[test]
    fn test_write_header() {
        let mut buf = Vec::new();
        let mut writer = BinaryWriter::new(&mut buf, 7400, false);
        writer.write_header().unwrap();

        assert_eq!(&buf[0..22], &BINARY_MAGIC[..]);
        assert_eq!(buf[22], 0); // Endian flag
        assert_eq!(buf.len(), 27);
    }

    #[test]
    fn test_write_simple_property() {
        let mut buf = Vec::new();
        let writer = BinaryWriter::new(Vec::new(), 7400, false);

        let value = Value::Int32(42);
        writer.write_property_to_buffer(&mut buf, &value).unwrap();

        assert_eq!(buf[0], b'I'); // Type code
        assert_eq!(buf.len(), 5); // 1 type + 4 bytes
    }

    #[test]
    fn test_write_node_simple() {
        let mut buf = Vec::new();
        let mut writer = BinaryWriter::new(&mut buf, 7400, false);

        let mut node = FbxNode::new("Test");
        node.values.push(Value::Int32(123));

        writer.write_node(&node, 0).unwrap();

        assert!(buf.len() > 0);
    }
}
