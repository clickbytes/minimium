use crate::operation::{CatalogEntry, OperationId, ParameterSpec, TypeRef};
#[derive(Clone, Debug)]
pub struct SignatureHelp {
    pub operation: OperationId,
    pub arguments: Vec<ParameterSpec>,
    pub active: usize,
    pub result: TypeRef,
    pub truncated: bool,
}
pub(super) fn signature_help(entry: &CatalogEntry, active: usize) -> SignatureHelp {
    SignatureHelp {
        operation: entry.spec.id.clone(),
        arguments: entry.spec.arguments.iter().take(32).copied().collect(),
        active,
        result: entry.spec.result,
        truncated: entry.spec.arguments.len() > 32,
    }
}
