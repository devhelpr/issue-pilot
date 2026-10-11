use serde_json::{json, Value};
fn strings() -> Value {
    json!({"type":"array","items":{"type":"string"}})
}
fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
pub fn specification() -> Value {
    let task = object(
        json!({"id":{"type":"string"},"repositoryId":{"type":"integer"},"title":{"type":"string"},"instructions":{"type":"string"},"dependencies":strings(),"acceptanceCriteria":strings()}),
        &[
            "id",
            "repositoryId",
            "title",
            "instructions",
            "dependencies",
            "acceptanceCriteria",
        ],
    );
    let check = object(
        json!({"repositoryId":{"type":"integer"},"command":{"type":"string"}}),
        &["repositoryId", "command"],
    );
    object(
        json!({"goals":strings(),"exclusions":strings(),"acceptanceCriteria":strings(),"repositoryChanges":strings(),"tasks":{"type":"array","items":task},"checks":{"type":"array","items":check},"questions":strings()}),
        &[
            "goals",
            "exclusions",
            "acceptanceCriteria",
            "repositoryChanges",
            "tasks",
            "checks",
            "questions",
        ],
    )
}
pub fn review() -> Value {
    object(
        json!({"summary":{"type":"string"},"blockingFindings":strings(),"scopeChange":{"type":"boolean"}}),
        &["summary", "blockingFindings", "scopeChange"],
    )
}
