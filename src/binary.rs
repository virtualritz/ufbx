//! Binary FBX File Format Parser
//!
//! This module implements a streaming parser for the FBX binary file format.
//! It handles:
//! - Magic header validation and version detection
//! - Binary node structure parsing (32-bit and 64-bit variants)
//! - Type-safe property reading (integers, floats, strings, arrays)
//! - DEFLATE-compressed array decompression
//! - Endianness conversion
//!
//! # Binary Format Overview
//!
//! FBX Binary files have the following structure:
//!
//! ```text
//! Header: 27 bytes
//!   - Magic: "Kaydara FBX Binary  \x00\x1a" (22 bytes)
//!   - Endian flag: 1 byte (0 = little-endian, 1 = big-endian)
//!   - Version: u32 (4 bytes, little-endian)
//!
//! Nodes: Recursive tree structure
//!   FBX < 7500 (32-bit offsets, 13 byte header):
//!     - end_offset: u32
//!     - num_properties: u32
//!     - property_list_len: u32
//!     - name_len: u8
//!     - name: [u8; name_len]
//!     - properties: [Property; num_properties]
//!     - children: [Node; ...] (until end_offset)
//!
//!   FBX >= 7500 (64-bit offsets, 25 byte header):
//!     - end_offset: u64
//!     - num_properties: u64
//!     - property_list_len: u64
//!     - name_len: u8
//!     - name: [u8; name_len]
//!     - properties: [Property; num_properties]
//!     - children: [Node; ...] (until end_offset)
//!
//! Properties (type code + data):
//!   - 'C', 'B', 'Z': u8
//!   - 'Y': i16
//!   - 'I': i32
//!   - 'L': i64
//!   - 'F': f32
//!   - 'D': f64
//!   - 'S', 'R': String (u32 length + bytes)
//!   - 'b', 'c', 'i', 'l', 'f', 'd': Array (see below)
//!
//! Arrays:
//!   - type: u8 (same as property type)
//!   - length: u32 (number of elements)
//!   - encoding: u32 (0 = raw, 1 = DEFLATE)
//!   - compressed_len: u32 (size of following data)
//!   - data: [u8; compressed_len]
//! ```

use byteorder::{LittleEndian, BigEndian, ReadBytesExt};
use flate2::read::ZlibDecoder;
use std::io::{self, Read, Seek, SeekFrom, BufReader};
use std::convert::TryInto;
use crate::error::{Error, Result};
use crate::types::FbxString;

// =============================================================================
// Constants
// =============================================================================

/// Magic bytes for FBX binary files: "Kaydara FBX Binary  \x00\x1a"
const BINARY_MAGIC: &[u8; 22] = b"Kaydara FBX Binary  \x00\x1a";

/// Size of the magic bytes
const BINARY_MAGIC_SIZE: usize = 22;

/// Total size of the binary header (magic + endian + version)
const BINARY_HEADER_SIZE: usize = 27;

/// FBX version threshold for 64-bit offsets
const FBX_VERSION_7500: u32 = 7500;

/// Node header size for versions < 7500 (32-bit offsets)
const NODE_HEADER_SIZE_32: usize = 13;

/// Node header size for versions >= 7500 (64-bit offsets)
const NODE_HEADER_SIZE_64: usize = 25;

/// Maximum node depth to prevent stack overflow
const MAX_NODE_DEPTH: u32 = 128;

/// Maximum non-array values per node
const MAX_NON_ARRAY_VALUES: usize = 16;

// =============================================================================
// Data Structures
// =============================================================================

/// Type of a property value
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    None,
    Number,
    String,
    Array,
}

/// A single property value (union of number and string)
#[derive(Debug, Clone)]
pub enum Value {
    Number { i: i64, f: f64 },
    String(FbxString),
}

impl Value {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Number { i, .. } => Some(*i),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number { f, .. } => Some(*f),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

/// Array property data
#[derive(Debug, Clone)]
pub struct ValueArray {
    pub data: ArrayData,
    pub array_type: char,
}

/// Type-safe array data
#[derive(Debug, Clone)]
pub enum ArrayData {
    Bool(Vec<bool>),
    I32(Vec<i32>),
    I64(Vec<i64>),
    F32(Vec<f32>),
    F64(Vec<f64>),
    U8(Vec<u8>),
    String(Vec<FbxString>),
}

impl ArrayData {
    pub fn len(&self) -> usize {
        match self {
            ArrayData::Bool(v) => v.len(),
            ArrayData::I32(v) => v.len(),
            ArrayData::I64(v) => v.len(),
            ArrayData::F32(v) => v.len(),
            ArrayData::F64(v) => v.len(),
            ArrayData::U8(v) => v.len(),
            ArrayData::String(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A node in the FBX document tree
#[derive(Debug, Clone)]
pub struct FbxNode {
    pub name: String,
    pub values: Vec<Value>,
    pub array: Option<ValueArray>,
    pub children: Vec<FbxNode>,
}

impl FbxNode {
    pub fn new(name: String) -> Self {
        Self {
            name,
            values: Vec::new(),
            array: None,
            children: Vec::new(),
        }
    }

    /// Find a child node by name
    pub fn find_child(&self, name: &str) -> Option<&FbxNode> {
        self.children.iter().find(|c| c.name == name)
    }

    /// Find a mutable child node by name
    pub fn find_child_mut(&mut self, name: &str) -> Option<&mut FbxNode> {
        self.children.iter_mut().find(|c| c.name == name)
    }
}

/// The complete FBX document
#[derive(Debug, Clone)]
pub struct FbxDocument {
    pub version: u32,
    pub big_endian: bool,
    pub root: FbxNode,
}

// =============================================================================
// Binary Parser
// =============================================================================

/// Binary FBX parser with streaming support
pub struct BinaryParser<R: Read> {
    reader: BufReader<R>,
    version: u32,
    file_big_endian: bool,
    local_big_endian: bool,
    offset: u64,
}

impl<R: Read> BinaryParser<R> {
    /// Create a new binary parser
    pub fn new(reader: R) -> Self {
        Self {
            reader: BufReader::new(reader),
            version: 0,
            file_big_endian: false,
            local_big_endian: cfg!(target_endian = "big"),
            offset: 0,
        }
    }

    /// Parse the entire FBX document
    pub fn parse(mut self) -> Result<FbxDocument> {
        // Read and validate header
        let version = self.read_header()?;

        // Create root node
        let mut root = FbxNode::new(String::new());

        // Parse all top-level nodes
        loop {
            let node = self.read_node(0)?;
            match node {
                Some(n) => root.children.push(n),
                None => break, // End sentinel reached
            }
        }

        Ok(FbxDocument {
            version,
            big_endian: self.file_big_endian,
            root,
        })
    }

    /// Read and validate the binary header
    fn read_header(&mut self) -> Result<u32> {
        // Read magic bytes
        let mut magic = [0u8; BINARY_MAGIC_SIZE];
        self.reader.read_exact(&mut magic)
            .map_err(|_| Error::unknown("Failed to read magic bytes"))?;

        if &magic != BINARY_MAGIC {
            return Err(Error::UnrecognizedFileFormat {
                filename: "binary FBX".to_string(),
            });
        }
        self.offset += BINARY_MAGIC_SIZE as u64;

        // Read endian flag
        let endian_flag = self.reader.read_u8()
            .map_err(|_| Error::unknown("Failed to read endian flag"))?;
        self.file_big_endian = endian_flag != 0;
        self.offset += 1;

        // Read version
        let version = if self.file_big_endian {
            self.reader.read_u32::<BigEndian>()?
        } else {
            self.reader.read_u32::<LittleEndian>()?
        };
        self.version = version;
        self.offset += 4;

        Ok(version)
    }

    /// Read a single node (returns None for end sentinel)
    fn read_node(&mut self, depth: u32) -> Result<Option<FbxNode>> {
        // Check depth limit
        if depth >= MAX_NODE_DEPTH {
            return Err(Error::NodeDepthLimit {
                depth: depth as usize,
                limit: MAX_NODE_DEPTH as usize,
            });
        }

        // Determine header size based on version
        let is_64bit = self.version >= FBX_VERSION_7500;

        // Read node header
        let (end_offset, num_properties, property_list_len, name_len) = if is_64bit {
            self.read_node_header_64()?
        } else {
            self.read_node_header_32()?
        };

        // Check for end sentinel (all zeros)
        if end_offset == 0 && name_len == 0 {
            return Ok(None);
        }

        // Read node name
        let name = self.read_name(name_len)?;

        let mut node = FbxNode::new(name);

        // Read properties
        let properties_start = self.offset;
        for _ in 0..num_properties {
            // Read type code
            let type_code = self.reader.read_u8()? as char;
            self.offset += 1;

            match type_code {
                // Array property types (lowercase)
                'b' | 'c' | 'i' | 'l' | 'f' | 'd' => {
                    // Read array directly (type_code already consumed)
                    let array = self.read_array(type_code)?;
                    node.array = Some(array);
                }
                // Scalar property types - read inline
                'C' | 'B' | 'Z' => {
                    let val = self.reader.read_u8()?;
                    self.offset += 1;
                    if node.values.len() < MAX_NON_ARRAY_VALUES {
                        node.values.push(Value::Number { i: val as i64, f: val as f64 });
                    }
                }
                'Y' => {
                    let val = if self.file_big_endian {
                        self.reader.read_i16::<BigEndian>()?
                    } else {
                        self.reader.read_i16::<LittleEndian>()?
                    };
                    self.offset += 2;
                    if node.values.len() < MAX_NON_ARRAY_VALUES {
                        node.values.push(Value::Number { i: val as i64, f: val as f64 });
                    }
                }
                'I' => {
                    let val = if self.file_big_endian {
                        self.reader.read_i32::<BigEndian>()?
                    } else {
                        self.reader.read_i32::<LittleEndian>()?
                    };
                    self.offset += 4;
                    if node.values.len() < MAX_NON_ARRAY_VALUES {
                        node.values.push(Value::Number { i: val as i64, f: val as f64 });
                    }
                }
                'L' => {
                    let val = if self.file_big_endian {
                        self.reader.read_i64::<BigEndian>()?
                    } else {
                        self.reader.read_i64::<LittleEndian>()?
                    };
                    self.offset += 8;
                    if node.values.len() < MAX_NON_ARRAY_VALUES {
                        node.values.push(Value::Number { i: val, f: val as f64 });
                    }
                }
                'F' => {
                    let val = if self.file_big_endian {
                        self.reader.read_f32::<BigEndian>()?
                    } else {
                        self.reader.read_f32::<LittleEndian>()?
                    };
                    self.offset += 4;
                    if node.values.len() < MAX_NON_ARRAY_VALUES {
                        node.values.push(Value::Number { i: val as i64, f: val as f64 });
                    }
                }
                'D' => {
                    let val = if self.file_big_endian {
                        self.reader.read_f64::<BigEndian>()?
                    } else {
                        self.reader.read_f64::<LittleEndian>()?
                    };
                    self.offset += 8;
                    if node.values.len() < MAX_NON_ARRAY_VALUES {
                        node.values.push(Value::Number { i: val as i64, f: val });
                    }
                }
                'S' | 'R' => {
                    let len = if self.file_big_endian {
                        self.reader.read_u32::<BigEndian>()?
                    } else {
                        self.reader.read_u32::<LittleEndian>()?
                    };
                    self.offset += 4;
                    let mut string_bytes = vec![0u8; len as usize];
                    self.reader.read_exact(&mut string_bytes)?;
                    self.offset += len as u64;
                    let string = String::from_utf8_lossy(&string_bytes).into_owned();
                    if node.values.len() < MAX_NON_ARRAY_VALUES {
                        node.values.push(Value::String(FbxString::new(string)));
                    }
                }
                _ => return Err(Error::unknown(format!("Unknown property type: {}", type_code))),
            }
        }

        // Skip any remaining property data
        let properties_end = properties_start + property_list_len;
        if self.offset < properties_end {
            self.skip_bytes((properties_end - self.offset) as usize)?;
        }

        // Read child nodes recursively
        while self.offset < end_offset {
            match self.read_node(depth + 1)? {
                Some(child) => node.children.push(child),
                None => break, // End sentinel
            }
        }

        // Ensure we're at the expected end offset
        if self.offset != end_offset && end_offset != 0 {
            return Err(Error::unknown(format!(
                "Node end offset mismatch: expected {}, got {}",
                end_offset, self.offset
            )));
        }

        Ok(Some(node))
    }

    /// Read 32-bit node header (FBX < 7500)
    fn read_node_header_32(&mut self) -> Result<(u64, u64, u64, u8)> {
        let end_offset = if self.file_big_endian {
            self.reader.read_u32::<BigEndian>()?
        } else {
            self.reader.read_u32::<LittleEndian>()?
        } as u64;

        let num_properties = if self.file_big_endian {
            self.reader.read_u32::<BigEndian>()?
        } else {
            self.reader.read_u32::<LittleEndian>()?
        } as u64;

        let property_list_len = if self.file_big_endian {
            self.reader.read_u32::<BigEndian>()?
        } else {
            self.reader.read_u32::<LittleEndian>()?
        } as u64;

        let name_len = self.reader.read_u8()?;

        self.offset += NODE_HEADER_SIZE_32 as u64;

        Ok((end_offset, num_properties, property_list_len, name_len))
    }

    /// Read 64-bit node header (FBX >= 7500)
    fn read_node_header_64(&mut self) -> Result<(u64, u64, u64, u8)> {
        let end_offset = if self.file_big_endian {
            self.reader.read_u64::<BigEndian>()?
        } else {
            self.reader.read_u64::<LittleEndian>()?
        };

        let num_properties = if self.file_big_endian {
            self.reader.read_u64::<BigEndian>()?
        } else {
            self.reader.read_u64::<LittleEndian>()?
        };

        let property_list_len = if self.file_big_endian {
            self.reader.read_u64::<BigEndian>()?
        } else {
            self.reader.read_u64::<LittleEndian>()?
        };

        let name_len = self.reader.read_u8()?;

        self.offset += NODE_HEADER_SIZE_64 as u64;

        Ok((end_offset, num_properties, property_list_len, name_len))
    }

    /// Read node name
    fn read_name(&mut self, len: u8) -> Result<String> {
        let mut name_bytes = vec![0u8; len as usize];
        self.reader.read_exact(&mut name_bytes)?;
        self.offset += len as u64;

        // Use lossy UTF-8 conversion - FBX files may contain non-UTF-8 data
        Ok(String::from_utf8_lossy(&name_bytes).into_owned())
    }

    /// Read a single property value
    fn read_property(&mut self) -> Result<Option<Value>> {
        let type_code = self.reader.read_u8()? as char;
        self.offset += 1;

        match type_code {
            // Boolean/byte values
            'C' | 'B' | 'Z' => {
                let val = self.reader.read_u8()?;
                self.offset += 1;
                Ok(Some(Value::Number {
                    i: val as i64,
                    f: val as f64,
                }))
            }

            // 16-bit integer
            'Y' => {
                let val = if self.file_big_endian {
                    self.reader.read_i16::<BigEndian>()?
                } else {
                    self.reader.read_i16::<LittleEndian>()?
                };
                self.offset += 2;
                Ok(Some(Value::Number {
                    i: val as i64,
                    f: val as f64,
                }))
            }

            // 32-bit integer
            'I' => {
                let val = if self.file_big_endian {
                    self.reader.read_i32::<BigEndian>()?
                } else {
                    self.reader.read_i32::<LittleEndian>()?
                };
                self.offset += 4;
                Ok(Some(Value::Number {
                    i: val as i64,
                    f: val as f64,
                }))
            }

            // 64-bit integer
            'L' => {
                let val = if self.file_big_endian {
                    self.reader.read_i64::<BigEndian>()?
                } else {
                    self.reader.read_i64::<LittleEndian>()?
                };
                self.offset += 8;
                Ok(Some(Value::Number {
                    i: val,
                    f: val as f64,
                }))
            }

            // 32-bit float
            'F' => {
                let val = if self.file_big_endian {
                    self.reader.read_f32::<BigEndian>()?
                } else {
                    self.reader.read_f32::<LittleEndian>()?
                };
                self.offset += 4;
                Ok(Some(Value::Number {
                    i: val as i64,
                    f: val as f64,
                }))
            }

            // 64-bit double
            'D' => {
                let val = if self.file_big_endian {
                    self.reader.read_f64::<BigEndian>()?
                } else {
                    self.reader.read_f64::<LittleEndian>()?
                };
                self.offset += 8;
                Ok(Some(Value::Number {
                    i: val as i64,
                    f: val,
                }))
            }

            // String/Raw string
            'S' | 'R' => {
                let len = if self.file_big_endian {
                    self.reader.read_u32::<BigEndian>()?
                } else {
                    self.reader.read_u32::<LittleEndian>()?
                };
                self.offset += 4;

                let mut string_bytes = vec![0u8; len as usize];
                self.reader.read_exact(&mut string_bytes)?;
                self.offset += len as u64;

                // Use lossy UTF-8 conversion - FBX files may contain non-UTF-8 data
                let string = String::from_utf8_lossy(&string_bytes).into_owned();

                Ok(Some(Value::String(FbxString::new(string))))
            }

            // Arrays - skip for now, would need context to know if we should parse
            'b' | 'c' | 'i' | 'l' | 'f' | 'd' => {
                // Read array header
                let _length = if self.file_big_endian {
                    self.reader.read_u32::<BigEndian>()?
                } else {
                    self.reader.read_u32::<LittleEndian>()?
                };
                let _encoding = if self.file_big_endian {
                    self.reader.read_u32::<BigEndian>()?
                } else {
                    self.reader.read_u32::<LittleEndian>()?
                };
                let compressed_len = if self.file_big_endian {
                    self.reader.read_u32::<BigEndian>()?
                } else {
                    self.reader.read_u32::<LittleEndian>()?
                };
                self.offset += 12;

                // Skip array data
                self.skip_bytes(compressed_len as usize)?;

                // Return None to skip this property
                Ok(None)
            }

            _ => Err(Error::unknown(format!("Unknown property type: {}", type_code))),
        }
    }

    /// Read and decompress an array property
    pub fn read_array(&mut self, type_code: char) -> Result<ValueArray> {
        // Read array header (already read type_code)
        let length = if self.file_big_endian {
            self.reader.read_u32::<BigEndian>()?
        } else {
            self.reader.read_u32::<LittleEndian>()?
        };

        let encoding = if self.file_big_endian {
            self.reader.read_u32::<BigEndian>()?
        } else {
            self.reader.read_u32::<LittleEndian>()?
        };

        let compressed_len = if self.file_big_endian {
            self.reader.read_u32::<BigEndian>()?
        } else {
            self.reader.read_u32::<LittleEndian>()?
        };
        self.offset += 12;

        // Read array data
        let data = match encoding {
            0 => {
                // Uncompressed
                self.read_array_uncompressed(type_code, length as usize)?
            }
            1 => {
                // DEFLATE compressed
                self.read_array_deflate(type_code, length as usize, compressed_len as usize)?
            }
            _ => {
                return Err(Error::unknown(format!("Unknown array encoding: {}", encoding)));
            }
        };

        Ok(ValueArray {
            data,
            array_type: type_code,
        })
    }

    /// Read uncompressed array data
    fn read_array_uncompressed(&mut self, type_code: char, length: usize) -> Result<ArrayData> {
        match type_code {
            'b' => {
                let mut data = vec![0u8; length];
                self.reader.read_exact(&mut data)?;
                self.offset += length as u64;
                Ok(ArrayData::Bool(data.into_iter().map(|b| b != 0).collect()))
            }
            'c' => {
                let mut data = vec![0u8; length];
                self.reader.read_exact(&mut data)?;
                self.offset += length as u64;
                Ok(ArrayData::U8(data))
            }
            'i' => {
                let mut data = vec![0i32; length];
                if self.file_big_endian {
                    self.reader.read_i32_into::<BigEndian>(&mut data)?;
                } else {
                    self.reader.read_i32_into::<LittleEndian>(&mut data)?;
                }
                self.offset += (length * 4) as u64;
                Ok(ArrayData::I32(data))
            }
            'l' => {
                let mut data = vec![0i64; length];
                if self.file_big_endian {
                    self.reader.read_i64_into::<BigEndian>(&mut data)?;
                } else {
                    self.reader.read_i64_into::<LittleEndian>(&mut data)?;
                }
                self.offset += (length * 8) as u64;
                Ok(ArrayData::I64(data))
            }
            'f' => {
                let mut data = vec![0f32; length];
                if self.file_big_endian {
                    self.reader.read_f32_into::<BigEndian>(&mut data)?;
                } else {
                    self.reader.read_f32_into::<LittleEndian>(&mut data)?;
                }
                self.offset += (length * 4) as u64;
                Ok(ArrayData::F32(data))
            }
            'd' => {
                let mut data = vec![0f64; length];
                if self.file_big_endian {
                    self.reader.read_f64_into::<BigEndian>(&mut data)?;
                } else {
                    self.reader.read_f64_into::<LittleEndian>(&mut data)?;
                }
                self.offset += (length * 8) as u64;
                Ok(ArrayData::F64(data))
            }
            _ => Err(Error::unknown(format!("Unknown array type: {}", type_code))),
        }
    }

    /// Read DEFLATE-compressed array data
    fn read_array_deflate(&mut self, type_code: char, length: usize, compressed_len: usize) -> Result<ArrayData> {
        // Read compressed bytes
        let mut compressed = vec![0u8; compressed_len];
        self.reader.read_exact(&mut compressed)?;
        self.offset += compressed_len as u64;

        // Decompress using zlib
        let mut decoder = ZlibDecoder::new(&compressed[..]);

        match type_code {
            'b' | 'c' => {
                let mut decompressed = vec![0u8; length];
                decoder.read_exact(&mut decompressed)
                    .map_err(|_| Error::unknown("DEFLATE decompression failed"))?;

                if type_code == 'b' {
                    Ok(ArrayData::Bool(decompressed.into_iter().map(|b| b != 0).collect()))
                } else {
                    Ok(ArrayData::U8(decompressed))
                }
            }
            'i' => {
                let byte_len = length * 4;
                let mut decompressed = vec![0u8; byte_len];
                decoder.read_exact(&mut decompressed)
                    .map_err(|_| Error::unknown("DEFLATE decompression failed"))?;

                let mut data = vec![0i32; length];
                let mut cursor = io::Cursor::new(decompressed);
                if self.file_big_endian {
                    cursor.read_i32_into::<BigEndian>(&mut data)?;
                } else {
                    cursor.read_i32_into::<LittleEndian>(&mut data)?;
                }
                Ok(ArrayData::I32(data))
            }
            'l' => {
                let byte_len = length * 8;
                let mut decompressed = vec![0u8; byte_len];
                decoder.read_exact(&mut decompressed)
                    .map_err(|_| Error::unknown("DEFLATE decompression failed"))?;

                let mut data = vec![0i64; length];
                let mut cursor = io::Cursor::new(decompressed);
                if self.file_big_endian {
                    cursor.read_i64_into::<BigEndian>(&mut data)?;
                } else {
                    cursor.read_i64_into::<LittleEndian>(&mut data)?;
                }
                Ok(ArrayData::I64(data))
            }
            'f' => {
                let byte_len = length * 4;
                let mut decompressed = vec![0u8; byte_len];
                decoder.read_exact(&mut decompressed)
                    .map_err(|_| Error::unknown("DEFLATE decompression failed"))?;

                let mut data = vec![0f32; length];
                let mut cursor = io::Cursor::new(decompressed);
                if self.file_big_endian {
                    cursor.read_f32_into::<BigEndian>(&mut data)?;
                } else {
                    cursor.read_f32_into::<LittleEndian>(&mut data)?;
                }
                Ok(ArrayData::F32(data))
            }
            'd' => {
                let byte_len = length * 8;
                let mut decompressed = vec![0u8; byte_len];
                decoder.read_exact(&mut decompressed)
                    .map_err(|_| Error::unknown("DEFLATE decompression failed"))?;

                let mut data = vec![0f64; length];
                let mut cursor = io::Cursor::new(decompressed);
                if self.file_big_endian {
                    cursor.read_f64_into::<BigEndian>(&mut data)?;
                } else {
                    cursor.read_f64_into::<LittleEndian>(&mut data)?;
                }
                Ok(ArrayData::F64(data))
            }
            _ => Err(Error::unknown(format!("Unknown array type: {}", type_code))),
        }
    }

    /// Skip bytes in the stream
    fn skip_bytes(&mut self, count: usize) -> Result<()> {
        let mut buf = vec![0u8; count.min(8192)];
        let mut remaining = count;

        while remaining > 0 {
            let to_read = remaining.min(buf.len());
            self.reader.read_exact(&mut buf[..to_read])?;
            remaining -= to_read;
        }

        self.offset += count as u64;
        Ok(())
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Check if data starts with FBX binary magic
pub fn is_binary_fbx(data: &[u8]) -> bool {
    data.len() >= BINARY_MAGIC_SIZE && &data[..BINARY_MAGIC_SIZE] == BINARY_MAGIC
}

/// Parse FBX from a byte slice
pub fn parse_binary(data: &[u8]) -> Result<FbxDocument> {
    let cursor = io::Cursor::new(data);
    let parser = BinaryParser::new(cursor);
    parser.parse()
}

/// Parse FBX from a file
pub fn parse_binary_file(path: &str) -> Result<FbxDocument> {
    let file = std::fs::File::open(path)
        .map_err(|_| Error::file_not_found(path))?;
    let parser = BinaryParser::new(file);
    parser.parse()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_magic_detection() {
        let valid = b"Kaydara FBX Binary  \x00\x1a";
        assert!(is_binary_fbx(valid));

        let invalid = b"Not FBX binary";
        assert!(!is_binary_fbx(invalid));
    }

    #[test]
    fn test_binary_header() {
        // Create a minimal valid header
        let mut data = Vec::new();
        data.extend_from_slice(BINARY_MAGIC);
        data.push(0); // Little-endian
        data.extend_from_slice(&7500u32.to_le_bytes()); // Version 7500

        let cursor = io::Cursor::new(data);
        let mut parser = BinaryParser::new(cursor);

        let version = parser.read_header().unwrap();
        assert_eq!(version, 7500);
        assert!(!parser.file_big_endian);
    }

    #[test]
    fn test_node_end_sentinel() {
        let mut data = Vec::new();
        data.extend_from_slice(BINARY_MAGIC);
        data.push(0); // Little-endian
        data.extend_from_slice(&7500u32.to_le_bytes()); // Version

        // End sentinel (25 bytes of zeros for 64-bit version)
        data.extend_from_slice(&[0u8; 25]);

        let cursor = io::Cursor::new(data);
        let mut parser = BinaryParser::new(cursor);
        parser.read_header().unwrap();

        let node = parser.read_node(0).unwrap();
        assert!(node.is_none());
    }

    #[test]
    fn test_property_parsing() {
        let mut data = Vec::new();

        // Integer property 'I'
        data.push(b'I');
        data.extend_from_slice(&42i32.to_le_bytes());

        let cursor = io::Cursor::new(data);
        let mut parser = BinaryParser::new(cursor);
        parser.file_big_endian = false;

        let prop = parser.read_property().unwrap().unwrap();
        match prop {
            Value::Number { i, .. } => assert_eq!(i, 42),
            _ => panic!("Expected number"),
        }
    }

    #[test]
    fn test_string_property() {
        let mut data = Vec::new();

        // String property 'S'
        let test_str = "Hello";
        data.push(b'S');
        data.extend_from_slice(&(test_str.len() as u32).to_le_bytes());
        data.extend_from_slice(test_str.as_bytes());

        let cursor = io::Cursor::new(data);
        let mut parser = BinaryParser::new(cursor);
        parser.file_big_endian = false;

        let prop = parser.read_property().unwrap().unwrap();
        match prop {
            Value::String(s) => assert_eq!(s.as_str(), "Hello"),
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_depth_limit() {
        let cursor = io::Cursor::new(Vec::new());
        let mut parser = BinaryParser::new(cursor);

        // Try to parse at max depth
        let result = parser.read_node(MAX_NODE_DEPTH);
        assert!(result.is_err());
    }
}
