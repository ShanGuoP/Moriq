use super::*;
use crate::{
    components::*,
    manage::Panel,
    theme::{input, secondary},
};
use iced::widget::{Column, button, checkbox, container, row, text_input};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    All,
    Any,
    Name,
    Notes,
    Tag,
    Format,
    Rating,
    Favorite,
    Raw,
    Location,
}
impl Kind {
    const ALL: [Self; 10] = [
        Self::All,
        Self::Any,
        Self::Name,
        Self::Notes,
        Self::Tag,
        Self::Format,
        Self::Rating,
        Self::Favorite,
        Self::Raw,
        Self::Location,
    ];
    fn rule(self) -> Rule {
        match self {
            Self::All => Rule::All(vec![Rule::Favorite]),
            Self::Any => Rule::Any(vec![Rule::Favorite]),
            Self::Name => Rule::Name(String::new()),
            Self::Notes => Rule::Notes(String::new()),
            Self::Tag => Rule::Tag(String::new()),
            Self::Format => Rule::Format("jpg".into()),
            Self::Rating => Rule::Rating(4),
            Self::Favorite => Rule::Favorite,
            Self::Raw => Rule::Raw,
            Self::Location => Rule::HasLocation,
        }
    }
}
impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::All => "全部满足",
            Self::Any => "任一满足",
            Self::Name => "文件名包含",
            Self::Notes => "备注包含",
            Self::Tag => "精确标签",
            Self::Format => "格式",
            Self::Rating => "最低评分",
            Self::Favorite => "已收藏",
            Self::Raw => "RAW 格式",
            Self::Location => "有补充地点",
        })
    }
}
#[derive(Debug, Clone)]
pub enum RuleMessage {
    Open,
    Set(Vec<usize>, Kind),
    Text(Vec<usize>, String),
    Negate(Vec<usize>),
    Add(Vec<usize>),
    Remove(Vec<usize>),
    Apply,
    Clear,
}
fn at<'a>(rule: &'a mut Rule, path: &[usize]) -> Option<&'a mut Rule> {
    if path.is_empty() {
        return Some(rule);
    }
    match rule {
        Rule::All(items) | Rule::Any(items) => at(items.get_mut(path[0])?, &path[1..]),
        Rule::Not(inner) if path[0] == 0 => at(inner, &path[1..]),
        _ => None,
    }
}
fn budget(rule: &Rule, depth: usize, count: &mut usize) -> bool {
    *count += 1;
    depth <= 8
        && *count <= 64
        && match rule {
            Rule::All(items) | Rule::Any(items) => {
                items.iter().all(|r| budget(r, depth + 1, count))
            }
            Rule::Not(r) => budget(r, depth + 1, count),
            _ => true,
        }
}
impl App {
    pub(super) fn rule_message(&mut self, event: RuleMessage) -> Task<Message> {
        let language = self.preferences.language;
        use RuleMessage::*;
        match event {
            Open => {
                self.manager.rule_in_filter = self.manager.panel == Some(Panel::Filters);
                self.manager.rule_draft = self
                    .query
                    .rule
                    .clone()
                    .or(Some(Rule::All(vec![Rule::Favorite])));
                if self.manager.rule_in_filter {
                    self.manager.rule_draft = self
                        .manager
                        .filter
                        .rule
                        .clone()
                        .or(Some(Rule::All(vec![Rule::Favorite])));
                }
                self.manager.panel = Some(Panel::Rules);
            }
            Apply => {
                if let Some(rule) = &self.manager.rule_draft
                    && let Err(e) = rule.validate()
                {
                    self.error = Some(e.into());
                    return Task::none();
                }
                if self.manager.rule_in_filter {
                    self.manager.filter.rule = self.manager.rule_draft.clone();
                    self.manager.panel = Some(Panel::Filters);
                    return Task::none();
                }
                self.query.rule = self.manager.rule_draft.clone();
                self.screen = Screen::Assets;
                self.manager.panel = None;
                return self.reset_query();
            }
            Clear => {
                if self.manager.rule_in_filter {
                    self.manager.filter.rule = None;
                    self.manager.panel = Some(Panel::Filters);
                    return Task::none();
                }
                self.query.rule = None;
                self.manager.rule_draft = None;
                self.manager.panel = None;
                return self.reset_query();
            }
            edit => {
                let Some(mut draft) = self.manager.rule_draft.clone() else {
                    return Task::none();
                };
                match edit {
                    Set(path, kind) => {
                        if let Some(r) = at(&mut draft, &path) {
                            *r = kind.rule();
                        }
                    }
                    Text(path, value) => {
                        if let Some(r) = at(&mut draft, &path) {
                            match r {
                                Rule::Name(s) | Rule::Notes(s) | Rule::Tag(s) | Rule::Format(s)
                                    if value.len() <= 1024 =>
                                {
                                    *s = value
                                }
                                Rule::Rating(n) => {
                                    if let Ok(v) = value.parse::<u8>()
                                        && v <= 5
                                    {
                                        *n = v;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Negate(path) => {
                        if let Some(r) = at(&mut draft, &path) {
                            *r = match r.clone() {
                                Rule::Not(inner) => *inner,
                                other => Rule::Not(Box::new(other)),
                            };
                        }
                    }
                    Add(path) => {
                        if let Some(Rule::All(items) | Rule::Any(items)) = at(&mut draft, &path) {
                            items.push(Rule::Favorite);
                        }
                    }
                    Remove(mut path) => {
                        if let Some(index) = path.pop()
                            && let Some(Rule::All(items) | Rule::Any(items)) = at(&mut draft, &path)
                            && items.len() > 1
                            && index < items.len()
                        {
                            items.remove(index);
                        }
                    }
                    _ => {}
                }
                if budget(&draft, 0, &mut 0) {
                    self.manager.rule_draft = Some(draft);
                } else {
                    self.error = Some(tr!(language, "组合规则最多 8 层、64 个条件及分组" => "Combined rules support up to 8 levels and 64 conditions or groups").into());
                }
            }
        }
        Task::none()
    }
    pub(super) fn rules_panel(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        let mut view=Column::new().spacing(12).push(muted(tr!(language, "这些组合条件与现有筛选同时生效。应用后可保存为智能集合；地点条件仅检查手动补充的地点。" => "Combined rules apply alongside current filters. Save them as a smart collection after applying. Location conditions check only manually added locations.")));
        if let Some(rule) = &self.manager.rule_draft {
            view = view.push(node(language, rule, vec![], false));
        }
        view.push(
            row![
                button(tr!(language, "移除组合条件" => "Remove combined rules"))
                    .style(secondary)
                    .on_press(Message::Rules(RuleMessage::Clear)),
                button(tr!(language, "应用条件" => "Apply rules"))
                    .style(secondary)
                    .on_press(Message::Rules(RuleMessage::Apply))
            ]
            .spacing(12),
        )
        .into()
    }
}
fn node(
    language: Language,
    rule: &Rule,
    path: Vec<usize>,
    removable: bool,
) -> Element<'_, Message> {
    let (inner, negated, inner_path) = if let Rule::Not(inner) = rule {
        let mut p = path.clone();
        p.push(0);
        (inner.as_ref(), true, p)
    } else {
        (rule, false, path.clone())
    };
    let kind = match inner {
        Rule::All(_) => Kind::All,
        Rule::Any(_) => Kind::Any,
        Rule::Name(_) => Kind::Name,
        Rule::Notes(_) => Kind::Notes,
        Rule::Tag(_) => Kind::Tag,
        Rule::Format(_) => Kind::Format,
        Rule::Rating(_) => Kind::Rating,
        Rule::Favorite => Kind::Favorite,
        Rule::Raw => Kind::Raw,
        _ => Kind::Location,
    };
    let set = inner_path.clone();
    let negate = path.clone();
    let mut header = row![
        localized_choice(language, Kind::ALL.to_vec(), Some(kind), move |k| {
            Message::Rules(RuleMessage::Set(set.clone(), k))
        }),
        checkbox(negated)
            .label(tr!(language, "排除" => "Exclude"))
            .on_toggle(move |_| Message::Rules(RuleMessage::Negate(negate.clone())))
    ]
    .spacing(10);
    if removable {
        header = header.push(
            button(tr!(language, "移除" => "Remove"))
                .style(secondary)
                .on_press(Message::Rules(RuleMessage::Remove(path))),
        );
    }
    let mut view = Column::new().spacing(8).push(header);
    match inner {
        Rule::All(items) | Rule::Any(items) => {
            for (i, item) in items.iter().enumerate() {
                let mut p = inner_path.clone();
                p.push(i);
                view = view.push(node(language, item, p, items.len() > 1));
            }
            view = view.push(
                button(tr!(language, "添加条件 / 分组" => "Add condition / group"))
                    .style(secondary)
                    .on_press(Message::Rules(RuleMessage::Add(inner_path))),
            );
        }
        Rule::Name(s) | Rule::Notes(s) | Rule::Tag(s) | Rule::Format(s) => {
            view = view.push(
                text_input(tr!(language, "输入条件" => "Enter condition"), s)
                    .style(input)
                    .on_input(move |v| Message::Rules(RuleMessage::Text(inner_path.clone(), v))),
            )
        }
        Rule::Rating(n) => {
            view = view.push(
                text_input("0～5", &n.to_string())
                    .style(input)
                    .on_input(move |v| Message::Rules(RuleMessage::Text(inner_path.clone(), v))),
            )
        }
        Rule::Not(_) => view = view.push(node(language, inner, inner_path, false)),
        _ => {}
    }
    container(view).padding(10).into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn nested_edit_paths_preserve_siblings_and_enforce_tree_budget() {
        let mut draft = Rule::All(vec![
            Rule::Not(Box::new(Rule::Any(vec![Rule::Favorite, Rule::Raw]))),
            Rule::Rating(4),
        ]);
        *at(&mut draft, &[0, 0, 1]).unwrap() = Rule::Name("照片".into());
        assert_eq!(at(&mut draft, &[1]), Some(&mut Rule::Rating(4)));
        assert!(at(&mut draft, &[9]).is_none());
        assert!(budget(&draft, 0, &mut 0));
        assert!(draft.validate().is_ok());
        assert!(!budget(&Rule::All(vec![Rule::Favorite; 64]), 0, &mut 0));
        let mut deep = Rule::Favorite;
        for _ in 0..9 {
            deep = Rule::Not(Box::new(deep));
        }
        assert!(!budget(&deep, 0, &mut 0));
    }
}
