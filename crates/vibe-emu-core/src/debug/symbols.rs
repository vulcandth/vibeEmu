use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolEntry {
    pub bank: u8,
    pub addr: u16,
    pub name: String,
}

#[derive(Debug, Clone, Default)]
pub struct SymbolTable {
    entries: Vec<SymbolEntry>,
    by_addr: BTreeMap<(u8, u16), Vec<usize>>,
    by_name: BTreeMap<String, usize>,
}

impl SymbolTable {
    pub fn from_file(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = File::open(path)?;
        Self::from_reader(BufReader::new(file))
    }

    pub fn from_reader<R: BufRead>(reader: R) -> io::Result<Self> {
        let mut table = SymbolTable::default();
        for line in reader.lines() {
            let line = line?;
            table.parse_line(&line);
        }
        Ok(table)
    }

    fn parse_line(&mut self, line: &str) {
        let mut trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(';') {
            return;
        }

        if let Some((prefix, _comment)) = trimmed.split_once(';') {
            trimmed = prefix.trim_end();
        }

        if trimmed.is_empty() {
            return;
        }

        let mut parts = trimmed.split_whitespace();
        let loc = match parts.next() {
            Some(loc) => loc,
            None => return,
        };
        let name = parts.collect::<Vec<_>>().join(" ");
        if name.is_empty() {
            return;
        }

        let (bank_str, addr_str) = match loc.split_once(':') {
            Some(pair) => pair,
            None => return,
        };
        let Ok(bank) = u8::from_str_radix(bank_str, 16) else {
            return;
        };
        let Ok(addr) = u16::from_str_radix(addr_str, 16) else {
            return;
        };

        self.push(SymbolEntry { bank, addr, name });
    }

    fn push(&mut self, entry: SymbolEntry) {
        let idx = self.entries.len();
        self.by_addr
            .entry((entry.bank, entry.addr))
            .or_default()
            .push(idx);
        self.by_name.entry(entry.name.clone()).or_insert(idx);
        self.entries.push(entry);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &SymbolEntry> {
        self.entries.iter()
    }

    pub fn symbols_at(&self, bank: u8, addr: u16) -> impl Iterator<Item = &SymbolEntry> {
        self.by_addr
            .get(&(bank, addr))
            .into_iter()
            .flat_map(|idxs| idxs.iter().map(|&i| &self.entries[i]))
    }

    pub fn lookup(&self, name: &str) -> Option<(u8, u16)> {
        self.by_name
            .get(name)
            .map(|&idx| (self.entries[idx].bank, self.entries[idx].addr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn parses_basic_rgbds_symbols() {
        let sym = b"; comment\n00:0100 Start\n01:2345 Label With Spaces\n00:0100 Duplicate ; inline comment\n";
        let table = SymbolTable::from_reader(Cursor::new(&sym[..])).expect("parse symbol file");

        assert_eq!(table.len(), 3);
        assert_eq!(table.lookup("Start"), Some((0x00, 0x0100)));
        assert_eq!(table.lookup("Label With Spaces"), Some((0x01, 0x2345)));

        let entries = table
            .symbols_at(0x00, 0x0100)
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(entries, vec!["Start", "Duplicate"]);
    }

    #[test]
    fn ignores_invalid_lines() {
        let sym = b"\n; comment\nnot even close\nZZ:HHHH Bad\n02:FFF0 MissingName \n";
        let table = SymbolTable::from_reader(Cursor::new(&sym[..])).expect("parse symbol file");

        assert_eq!(table.len(), 1);
        assert_eq!(table.lookup("MissingName"), Some((0x02, 0xFFF0)));
        assert!(table.symbols_at(0x00, 0x0000).next().is_none());
    }
}
