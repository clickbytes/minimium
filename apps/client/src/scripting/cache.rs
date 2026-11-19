//! ASTs stay on the one script worker. Cached source bytes also disambiguate hashes.
use rhai::AST;

pub(crate) const MAX_ENTRIES: usize = 16;
pub(crate) const MAX_SOURCE_BYTES: usize = 256 * 1024;
pub(crate) const ADAPTER_VERSION: u16 = 1;
pub(crate) const RHAI_VERSION: &str = "1.27.0";

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct CacheKey {
    source_hash: u64,
    schema: u64,
    adapter: u16,
    rhai: &'static str,
    compiler: [u64; 9],
}

impl CacheKey {
    pub(crate) fn for_source(source: &[u8], schema: u64) -> Self {
        let mut hash = 0xcbf29ce484222325_u64;
        for byte in source {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
        Self {
            source_hash: hash,
            schema,
            adapter: ADAPTER_VERSION,
            rhai: RHAI_VERSION,
            compiler: [
                0, // OptimizationLevel::None
                super::limits::MAX_OPERATIONS,
                super::limits::MAX_CALL_LEVELS as u64,
                super::limits::MAX_EXPR_DEPTH as u64,
                super::limits::MAX_FUNCTION_EXPR_DEPTH as u64,
                super::limits::MAX_STRING_BYTES as u64,
                super::limits::MAX_ARRAY_SIZE as u64,
                super::limits::MAX_MAP_SIZE as u64,
                super::limits::MAX_VARIABLES as u64,
            ],
        }
    }
}

struct Entry {
    key: CacheKey,
    source: Vec<u8>,
    ast: AST,
    touched: u64,
}

#[derive(Default)]
pub(crate) struct AstCache {
    entries: Vec<Entry>,
    source_bytes: usize,
    clock: u64,
    schema: Option<u64>,
}

impl AstCache {
    pub(crate) fn get(&mut self, key: &CacheKey, source: &[u8]) -> Option<AST> {
        self.ensure_schema(key.schema);
        let index = self
            .entries
            .iter()
            .position(|entry| entry.key == *key && entry.source == source)?;
        self.clock = self.clock.saturating_add(1);
        self.entries[index].touched = self.clock;
        Some(self.entries[index].ast.clone())
    }

    pub(crate) fn insert(&mut self, key: CacheKey, source: &[u8], ast: AST) {
        self.ensure_schema(key.schema);
        if source.len() > super::limits::SOURCE_BYTES || source.len() > MAX_SOURCE_BYTES {
            return;
        }
        self.clock = self.clock.saturating_add(1);
        while self.entries.len() >= MAX_ENTRIES
            || self.source_bytes.saturating_add(source.len()) > MAX_SOURCE_BYTES
        {
            let Some(index) = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, entry)| entry.touched)
                .map(|(index, _)| index)
            else {
                break;
            };
            let evicted = self.entries.remove(index);
            self.source_bytes -= evicted.source.len();
        }
        self.source_bytes += source.len();
        self.entries.push(Entry {
            key,
            source: source.to_vec(),
            ast,
            touched: self.clock,
        });
    }

    pub(crate) fn occupancy(&self) -> (usize, usize) {
        (self.entries.len(), self.source_bytes)
    }

    fn ensure_schema(&mut self, schema: u64) {
        if self.schema != Some(schema) {
            self.entries.clear();
            self.source_bytes = 0;
            self.schema = Some(schema);
        }
    }
}
