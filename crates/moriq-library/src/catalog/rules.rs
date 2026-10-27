use super::*;
use rusqlite::types::Value;

/// 把已校验的规则树编译为带参数的 SQL 条件，并追加绑定值。
pub(super) fn compile(rule: &Rule, values: &mut Vec<Value>) -> Result<String> {
    rule.validate().map_err(|e| Error::Invalid(e.into()))?;
    /// 递归生成规则节点对应的条件表达式，保持参数序号与值列表一致。
    fn emit(rule: &Rule, values: &mut Vec<Value>) -> String {
        match rule {
            Rule::All(items) | Rule::Any(items) => {
                let join = if matches!(rule, Rule::All(_)) {
                    " AND "
                } else {
                    " OR "
                };
                format!(
                    "({})",
                    items
                        .iter()
                        .map(|r| emit(r, values))
                        .collect::<Vec<_>>()
                        .join(join)
                )
            }
            Rule::Not(item) => format!("(NOT ({}))", emit(item, values)),
            Rule::Favorite => "a.favorite=1".into(),
            Rule::Raw => format!("v.format IN({})", native::raw_sql()),
            Rule::HasLocation => {
                "EXISTS(SELECT 1 FROM version_locations vl WHERE vl.version_id=v.id)".into()
            }
            leaf => {
                values.push(match leaf {
                    Rule::Name(s) | Rule::Notes(s) | Rule::Tag(s) => Value::Text(s.clone()),
                    Rule::Format(s) => Value::Text(s.to_ascii_lowercase()),
                    Rule::Rating(n) => Value::Integer(i64::from(*n)),
                    _ => unreachable!(),
                });
                let p = values.len();
                match leaf {
                    Rule::Name(_) => format!("instr(lower(a.name),lower(?{p}))>0"),
                    Rule::Notes(_) => format!("instr(lower(a.notes),lower(?{p}))>0"),
                    Rule::Tag(_) => format!(
                        "EXISTS(SELECT 1 FROM effective_asset_tags t WHERE t.asset_id=a.id AND t.name=?{p})"
                    ),
                    Rule::Format(_) => format!("v.format=?{p}"),
                    Rule::Rating(_) => format!("a.rating>=?{p}"),
                    _ => unreachable!(),
                }
            }
        }
    }
    Ok(emit(rule, values))
}
