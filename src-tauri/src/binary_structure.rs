use std::{collections::HashMap, fs::File, path::Path};

use object::{Object, ObjectSection, ObjectSymbol, read::{ReadCache, ReadRef, elf::{Dyn, ElfFile, ElfFile32, ElfFile64, FileHeader, ProgramHeader}}};
use pelite::{PeFile, Wrap, pe64::{imports::Import, exports::Export}};
use serde::Serialize;

use crate::command_error::CommandResult;

const TABLE_LIMIT: usize = 5_000;

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryStructureRow {
    name: String,
    address: Option<String>,
    offset: Option<String>,
    size: Option<String>,
    details: String,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryStructureTable {
    items: Vec<BinaryStructureRow>,
    truncated: bool,
}

impl BinaryStructureTable {
    fn push(&mut self, row: BinaryStructureRow) -> bool {
        if self.items.len() == TABLE_LIMIT {
            self.truncated = true;
            return false;
        }
        self.items.push(row);
        true
    }
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryStructureDocument {
    format: &'static str,
    bitness: u8,
    architecture: String,
    byte_order: &'static str,
    entry_point: String,
    image_base: Option<String>,
    file_type: String,
    sections: BinaryStructureTable,
    imports: BinaryStructureTable,
    exports: BinaryStructureTable,
    symbols: BinaryStructureTable,
    program_headers: BinaryStructureTable,
    dependencies: BinaryStructureTable,
}

fn hex(value: u64) -> String { format!("0x{value:X}") }
fn parse_error(error: impl std::fmt::Display) -> String { format!("无法解析二进制结构：{error}") }

fn elf_structure<'data, Elf: FileHeader, R: ReadRef<'data>>(
    file: ElfFile<'data, Elf, R>,
) -> Result<BinaryStructureDocument, String> {
    let endian = file.endian();
    let mut result = BinaryStructureDocument {
        format: "ELF",
        bitness: if file.is_64() { 64 } else { 32 },
        architecture: format!("{:?}", file.architecture()),
        byte_order: if file.is_little_endian() { "小端" } else { "大端" },
        entry_point: hex(file.entry()),
        file_type: format!("{:?}", file.elf_header().e_type(endian)),
        ..Default::default()
    };
    for section in file.sections().take(TABLE_LIMIT + 1) {
        let (offset, size) = section.file_range().unzip();
        if !result.sections.push(BinaryStructureRow {
            name: String::from_utf8_lossy(section.name_bytes().map_err(parse_error)?).into_owned(),
            address: Some(hex(section.address())),
            offset: offset.map(hex),
            size: Some(hex(section.size())),
            details: format!("{:?} · {:?} · 文件长度 {}", section.kind(), section.flags(), size.map(hex).unwrap_or_else(|| "无文件内容".into())),
        }) { break; }
    }
    for header in file.elf_program_headers().iter().take(TABLE_LIMIT + 1) {
        if !result.program_headers.push(BinaryStructureRow {
            name: format!("{:?}", header.p_type(endian)),
            address: Some(hex(header.p_vaddr(endian).into())),
            offset: Some(hex(header.p_offset(endian).into())),
            size: Some(hex(header.p_filesz(endian).into())),
            details: format!("{:?} · 内存长度 {} · 对齐 {}", header.p_flags(endian), hex(header.p_memsz(endian).into()), hex(header.p_align(endian).into())),
        }) { break; }
    }
    result.dependencies = elf_dependencies(&file)?;
    for symbol in file.dynamic_symbols().filter(|symbol| symbol.is_undefined() && (symbol.is_global() || symbol.is_weak())).take(TABLE_LIMIT + 1) {
        if !result.imports.push(BinaryStructureRow {
            name: String::from_utf8_lossy(symbol.name_bytes().map_err(parse_error)?).into_owned(),
            details: format!("{:?} · {:?}", symbol.kind(), symbol.scope()),
            ..Default::default()
        }) { break; }
    }
    for symbol in file.dynamic_symbols().filter(|symbol| symbol.is_definition() && (symbol.is_global() || symbol.is_weak())).take(TABLE_LIMIT + 1) {
        if !result.exports.push(elf_symbol_row(symbol)?) { break; }
    }
    // 没有常规符号表的 stripped ELF 使用其实际存在的动态符号表。
    let regular_symbols = file.symbol_table().is_some();
    let symbols = if regular_symbols { file.symbols() } else { file.dynamic_symbols() };
    for symbol in symbols.take(TABLE_LIMIT + 1) {
        let mut row = elf_symbol_row(symbol)?;
        row.details = format!("{} · {}", if regular_symbols { "常规符号表" } else { "动态符号表" }, row.details);
        if !result.symbols.push(row) { break; }
    }
    Ok(result)
}

fn elf_dependencies<'data, Elf: FileHeader, R: ReadRef<'data>>(
    file: &ElfFile<'data, Elf, R>,
) -> Result<BinaryStructureTable, String> {
    let endian = file.endian();
    let mut result = BinaryStructureTable::default();
    // 动态依赖从程序头读取，移除了节表的 ELF 也保留此信息。
    for header in file.elf_program_headers() {
        let Some(dynamic) = header.dynamic(endian, file.data()).map_err(parse_error)? else { continue };
        let entries = dynamic.iter().take_while(|entry| entry.d_tag(endian) != object::elf::DT_NULL);
        let needed = entries.clone().filter(|entry| entry.d_tag(endian) == object::elf::DT_NEEDED);
        if needed.clone().next().is_none() { continue; }
        let address = entries.clone().find(|entry| entry.d_tag(endian) == object::elf::DT_STRTAB)
            .ok_or("ELF 动态依赖缺少字符串表地址")?.val(endian);
        let size = entries.clone().find(|entry| entry.d_tag(endian) == object::elf::DT_STRSZ)
            .ok_or("ELF 动态依赖缺少字符串表长度")?.val(endian);
        let segment = file.elf_program_headers().iter().find(|segment| {
            let start: u64 = segment.p_vaddr(endian).into();
            let length: u64 = segment.p_filesz(endian).into();
            segment.p_type(endian) == object::elf::PT_LOAD && address.checked_sub(start)
                .is_some_and(|delta| delta < length && size <= length - delta)
        }).ok_or("ELF 动态字符串表超出文件段范围")?;
        let segment_address: u64 = segment.p_vaddr(endian).into();
        let segment_offset: u64 = segment.p_offset(endian).into();
        let start = segment_offset.checked_add(address - segment_address).ok_or("ELF 字符串表偏移溢出")?;
        let end = start.checked_add(size).ok_or("ELF 字符串表长度溢出")?;
        for entry in needed.take(TABLE_LIMIT + 1) {
            let offset = entry.val(endian);
            if offset >= size { return Err("ELF 动态依赖名称偏移无效".into()); }
            let name = file.data().read_bytes_at_until(start + offset..end, 0)
                .map_err(|_| "ELF 动态依赖名称无效")?;
            if !result.push(BinaryStructureRow {
                name: String::from_utf8_lossy(name).into_owned(),
                details: "DT_NEEDED".into(),
                ..Default::default()
            }) { return Ok(result); }
        }
    }
    Ok(result)
}

fn elf_symbol_row<'data>(symbol: impl ObjectSymbol<'data>) -> Result<BinaryStructureRow, String> {
    Ok(BinaryStructureRow {
        name: String::from_utf8_lossy(symbol.name_bytes().map_err(parse_error)?).into_owned(),
        address: Some(hex(symbol.address())),
        size: Some(hex(symbol.size())),
        details: format!("{:?} · {:?} · {:?}", symbol.kind(), symbol.scope(), symbol.section()),
        ..Default::default()
    })
}

fn pe_structure(bytes: &[u8]) -> Result<BinaryStructureDocument, String> {
    let pe = PeFile::from_bytes(bytes).map_err(parse_error)?;
    if pe.file_header().Characteristics & pelite::image::IMAGE_FILE_DLL == 0 {
        return Err("PE 文件未声明为 DLL".into());
    }
    let (bitness, entry, base) = match pe.optional_header() {
        Wrap::T32(header) => (32, header.AddressOfEntryPoint, u64::from(header.ImageBase)),
        Wrap::T64(header) => (64, header.AddressOfEntryPoint, header.ImageBase),
    };
    let mut result = BinaryStructureDocument {
        format: "PE",
        bitness,
        architecture: match pe.file_header().Machine {
            0x014c => "x86".into(), 0x8664 => "x64".into(), 0xaa64 => "ARM64".into(),
            0x01c0 | 0x01c4 => "ARM".into(), value => format!("Machine 0x{value:04X}"),
        },
        byte_order: "小端",
        entry_point: hex(u64::from(entry)),
        image_base: Some(hex(base)),
        file_type: format!("DLL · Characteristics 0x{:04X}", pe.file_header().Characteristics),
        ..Default::default()
    };
    for section in pe.section_headers().iter().take(TABLE_LIMIT + 1) {
        if !result.sections.push(BinaryStructureRow {
            name: String::from_utf8_lossy(section.name_bytes()).into_owned(),
            address: Some(hex(u64::from(section.VirtualAddress))),
            offset: Some(hex(u64::from(section.PointerToRawData))),
            size: Some(hex(u64::from(section.SizeOfRawData))),
            details: format!("RVA · 虚拟长度 {} · 标志 0x{:08X}", hex(u64::from(section.VirtualSize)), section.Characteristics),
        }) { break; }
    }
    match pe.imports() {
        Ok(imports) => {
            for descriptor in imports.iter().take(TABLE_LIMIT + 1) {
                if !result.dependencies.push(BinaryStructureRow {
                    name: descriptor.dll_name().map_err(parse_error)?.to_string(),
                    ..Default::default()
                }) { break; }
            }
            'libraries: for descriptor in imports.iter().take(TABLE_LIMIT) {
                let library = descriptor.dll_name().map_err(parse_error)?.to_string();
                for import in descriptor.int().map_err(parse_error)? {
                    let name = match import.map_err(parse_error)? {
                        Import::ByName { name, .. } => name.to_string(),
                        Import::ByOrdinal { ord } => format!("#{ord}"),
                    };
                    if !result.imports.push(BinaryStructureRow { name, details: library.clone(), ..Default::default() }) {
                        break 'libraries;
                    }
                }
            }
            result.imports.truncated |= result.dependencies.truncated;
        }
        Err(pelite::Error::Null) => {}
        Err(error) => return Err(parse_error(error)),
    }
    match pe.exports() {
        Ok(exports) => {
            let by = exports.by().map_err(parse_error)?;
            let functions: Vec<_> = by.functions().iter().enumerate()
                .filter(|(_, rva)| **rva != 0).take(TABLE_LIMIT + 1).collect();
            let mut names: HashMap<usize, Option<String>> = functions.iter().map(|(index, _)| (*index, None)).collect();
            // pelite::name_lookup 每次线性扫描名称表；一次扫描构建当前页名称索引。
            for (hint, index) in by.name_indices().iter().enumerate() {
                if let Some(name) = names.get_mut(&usize::from(*index)) {
                    if name.is_none() { *name = Some(by.name_of_hint(hint).map_err(parse_error)?.to_string()); }
                }
            }
            for (index, rva) in functions {
                let name = names.remove(&index).flatten()
                    .unwrap_or_else(|| format!("#{}", u64::from(by.image().Base) + index as u64));
                let (address, details) = match by.index(index).map_err(parse_error)? {
                    Export::Symbol(_) => (Some(hex(u64::from(*rva))), "RVA".into()),
                    Export::Forward(target) => (None, format!("转发至 {target}")),
                };
                if !result.exports.push(BinaryStructureRow { name, address, details, ..Default::default() }) { break; }
            }
        }
        Err(pelite::Error::Null) => {}
        Err(error) => return Err(parse_error(error)),
    }
    result.symbols = coff_symbols(bytes, pe.file_header().PointerToSymbolTable, pe.file_header().NumberOfSymbols)?;
    Ok(result)
}

fn slice(bytes: &[u8], offset: usize, length: usize) -> Result<&[u8], String> {
    let end = offset.checked_add(length).ok_or("二进制结构范围溢出")?;
    bytes.get(offset..end).ok_or_else(|| "二进制结构超出文件范围".into())
}

fn u32_le(bytes: &[u8]) -> Result<u32, String> {
    Ok(u32::from_le_bytes(bytes.try_into().map_err(|_| "COFF 字段长度无效")?))
}

fn coff_symbols(bytes: &[u8], offset: u32, count: u32) -> Result<BinaryStructureTable, String> {
    let mut result = BinaryStructureTable::default();
    if count == 0 { return Ok(result); }
    if offset == 0 { return Err("COFF 符号表偏移无效".into()); }
    let length = (count as usize).checked_mul(18).ok_or("COFF 符号表长度溢出")?;
    let records = slice(bytes, offset as usize, length)?;
    let strings_offset = (offset as usize).checked_add(length).ok_or("COFF 字符串表偏移溢出")?;
    let strings_length = u32_le(slice(bytes, strings_offset, 4)?)? as usize;
    if strings_length < 4 { return Err("COFF 字符串表长度无效".into()); }
    let strings = slice(bytes, strings_offset, strings_length)?;
    let mut index = 0usize;
    while index < count as usize {
        let record = slice(records, index * 18, 18)?;
        let name_bytes = if record[..4] == [0; 4] {
            let start = u32_le(&record[4..8])? as usize;
            if start < 4 { return Err("COFF 符号名偏移无效".into()); }
            let tail = strings.get(start..).ok_or("COFF 符号名超出字符串表")?;
            let end = tail.iter().position(|value| *value == 0).ok_or("COFF 符号名缺少终止符")?;
            &tail[..end]
        } else {
            let end = record[..8].iter().position(|value| *value == 0).unwrap_or(8);
            &record[..end]
        };
        if !result.push(BinaryStructureRow {
            name: String::from_utf8_lossy(name_bytes).into_owned(),
            address: Some(hex(u64::from(u32_le(&record[8..12])?))),
            details: format!("COFF 节内值 · 节 {} · 存储类别 {}", i16::from_le_bytes([record[12], record[13]]), record[16]),
            ..Default::default()
        }) { break; }
        index += 1 + usize::from(record[17]);
        if index > count as usize { return Err("COFF 辅助符号超出符号表".into()); }
    }
    Ok(result)
}

#[tauri::command]
pub async fn read_binary_structure(path: String, expected_modified_at: u64) -> CommandResult<BinaryStructureDocument> {
    tauri::async_runtime::spawn_blocking(move || parse_binary_structure(&path, expected_modified_at))
        .await.map_err(|error| format!("二进制结构后台解析任务失败：{error}"))?
}

fn parse_binary_structure(path: &str, expected_modified_at: u64) -> CommandResult<BinaryStructureDocument> {
    let canonical = Path::new(path).canonicalize().map_err(|error| format!("无法访问文件：{error}"))?;
    let file = File::open(&canonical).map_err(|error| format!("无法打开文件：{error}"))?;
    let metadata = file.metadata().map_err(|error| format!("无法读取文件信息：{error}"))?;
    if !metadata.is_file() { return Err("所选路径不是文件".into()); }
    if crate::modified_millis(&metadata) != expected_modified_at {
        return Err("文件已被外部修改，请重新打开".into());
    }
    let cache = ReadCache::new(file);
    let signature = (&cache).read_bytes_at(0, 4).map_err(|_| "无法读取二进制文件头")?;
    let result = if signature == b"\x7fELF" {
        let class = (&cache).read_bytes_at(4, 1).map_err(|_| "ELF 文件头不完整")?[0];
        match class {
            1 => elf_structure(ElfFile32::<object::Endianness, _>::parse(&cache).map_err(parse_error)?),
            2 => elf_structure(ElfFile64::<object::Endianness, _>::parse(&cache).map_err(parse_error)?),
            _ => Err("ELF 位数标识无效".into()),
        }
    } else if canonical.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case("dll")) {
        let map = pelite::FileMap::open(&canonical).map_err(|error| format!("无法映射 DLL：{error}"))?;
        pe_structure(map.as_ref())
    } else {
        Err("文件内容不是 ELF 或 DLL".into())
    }?;
    let after = canonical.metadata().map_err(|error| format!("无法复核文件信息：{error}"))?;
    if crate::modified_millis(&after) != expected_modified_at || after.len() != metadata.len() {
        return Err("文件在解析期间被外部修改，请重新打开".into());
    }
    Ok(result)
}
