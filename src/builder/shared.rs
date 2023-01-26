use super::builder::{EntityMethods, Entry};
use super::builder::{ASTEntry, Builder};
use super::comment::JSDocComment;
use super::namespace::CppItem;
use crate::config::Config;
use crate::{
    html::{Html, HtmlElement, HtmlList, HtmlText},
};
use clang::{
    Accessibility, Entity, EntityKind, Type, TypeKind,
};
use multipeek::{IteratorExt, MultiPeek};
use pulldown_cmark::{Event, Tag, CowStr};
use std::str::Chars;
use std::sync::Arc;

trait Surround<T> {
    fn surround(self, start: T, end: T) -> Self;
}

impl<T> Surround<T> for Vec<T> {
    fn surround(mut self, start: T, end: T) -> Self {
        self.insert(0, start);
        self.push(end);
        self
    }
}

trait InsertBetween<T, Sep: Fn() -> T> {
    fn insert_between(self, separator: Sep) -> Self;
}

impl<T, Sep: Fn() -> T> InsertBetween<T, Sep> for Vec<T> {
    fn insert_between(self, separator: Sep) -> Self {
        let mut res = Vec::new();
        let mut first = true;
        for item in self.into_iter() {
            if !first {
                res.push(separator());
            }
            first = false;
            res.push(item);
        }
        res
    }
}

fn fmt_type(entity: &Type, builder: &Builder) -> Html {
    let base = entity.get_pointee_type().unwrap_or(entity.to_owned());
    let decl = base.get_declaration();
    let link = decl.map(|decl| decl.docs_url(builder.config.clone()));
    let kind = decl
        .map(|decl| decl.get_kind())
        .unwrap_or(EntityKind::UnexposedDecl);

    let name: Html = decl
        .map(|decl| {
            HtmlList::new(
                decl.ancestorage()
                    .iter()
                    .map(|e| {
                        HtmlElement::new("span")
                            .with_class(match e.get_kind() {
                                EntityKind::Namespace => "namespace",
                                EntityKind::ClassDecl => "class",
                                EntityKind::ClassTemplate => "class",
                                EntityKind::StructDecl => "struct",
                                EntityKind::FunctionDecl => "fun",
                                EntityKind::TypedefDecl => "alias",
                                EntityKind::UsingDeclaration => "alias",
                                EntityKind::TypeAliasDecl => "alias",
                                EntityKind::EnumDecl => "enum",
                                _ => "type",
                            })
                            .with_class("name")
                            .with_child(HtmlText::new(e.get_name().unwrap_or("_".into())))
                            .into()
                    })
                    .collect::<Vec<_>>()
                    .insert_between(|| Html::span(&["scope"], "::")),
            )
            .into()
        })
        .unwrap_or_else(|| {
            HtmlElement::new("span")
                .with_class(if base.is_pod() {
                    "keyword"
                } else {
                    "template-param"
                })
                .with_class("name")
                .with_child(HtmlText::new(match base.get_kind() {
                    TypeKind::Void => "void".into(),
                    TypeKind::Bool => "bool".into(),
                    TypeKind::Long => "long".into(),
                    TypeKind::Auto => "auto".into(),
                    TypeKind::Int => "int".into(),
                    TypeKind::Short => "short".into(),
                    TypeKind::SChar | TypeKind::CharS => "char".into(),
                    TypeKind::UChar | TypeKind::CharU => "uchar".into(),
                    TypeKind::Float => "float".into(),
                    TypeKind::Double => "double".into(),
                    TypeKind::UInt => "uint".into(),
                    TypeKind::LongLong => "long long".into(),
                    _ => base.get_display_name(),
                }))
                .into()
        });
    
    HtmlElement::new("a")
        .with_class("entity")
        .with_class("type")
        .with_class_opt(entity.is_pod().then_some("keyword"))
        .with_class_opt(link.is_none().then_some("disabled"))
        .with_attr_opt("href", link.clone())
        .with_attr_opt(
            "onclick",
            link.map(|link| format!("return navigate('{link}'")),
        )
        .with_child(name)
        .with_child_opt(match kind {
            EntityKind::TypeAliasDecl | EntityKind::TypedefDecl => None,
            _ => base.get_template_argument_types().map(|types| {
                HtmlList::new(
                    types
                        .iter()
                        .map(|t| {
                            t.map(|t| fmt_type(&t, builder))
                                .unwrap_or(HtmlText::new("_unk").into())
                        })
                        .collect::<Vec<_>>()
                        .insert_between(|| {
                            HtmlElement::new("span")
                                .with_class("comma")
                                .with_class("space-after")
                                .with_child(HtmlText::new(","))
                                .into()
                        })
                        .surround(HtmlText::new("<").into(), HtmlText::new(">").into()),
                )
            }),
        })
        .with_child_opt(
            base.is_const_qualified()
                .then_some(Html::span(&["keyword", "space-before"], "const")),
        )
        .with_child_opt(match entity.get_kind() {
            TypeKind::LValueReference => Some::<Html>(HtmlText::new("&").into()),
            TypeKind::RValueReference => Some(HtmlText::new("&&").into()),
            TypeKind::Pointer => Some(HtmlText::new("*").into()),
            _ => None,
        })
        .into()
}

fn fmt_param(param: &Entity, builder: &Builder) -> Html {
    HtmlElement::new("div")
        .with_classes(&["entity", "var"])
        .with_child_opt(param.get_type().map(|t| fmt_type(&t, builder)))
        .with_child_opt(
            param
                .get_display_name()
                .map(|name| Html::span(&["name", "space-before"], &name)),
        )
        .into()
}

pub fn fmt_field(field: &Entity, builder: &Builder) -> Html {
    HtmlElement::new("details")
        .with_class("entity-desc")
        .with_child(
            HtmlElement::new("summary")
                .with_classes(&["entity", "var"])
                .with_child(fmt_param(field, builder))
                .with_child(HtmlText::new(";"))
        )
        .with_child(
            HtmlElement::new("div").with_child(
                field.get_comment()
                    .map(|s| JSDocComment::parse(s, builder).to_html(true))
                    .unwrap_or(Html::p("No description provided")),
            ),
        )
        .into()
}

pub fn fmt_fun_decl(fun: &Entity, builder: &Builder) -> Html {
    HtmlElement::new("details")
        .with_class("entity-desc")
        .with_child(
            HtmlElement::new("summary")
                .with_classes(&["entity", "fun"])
                .with_child_opt(
                    fun.is_static_method()
                        .then_some(Html::span(&["keyword", "space-after"], "static")),
                )
                .with_child_opt(
                    fun.is_virtual_method()
                        .then_some(Html::span(&["keyword", "space-after"], "virtual")),
                )
                .with_child_opt(fun.get_result_type().map(|t| fmt_type(&t, builder)))
                .with_child(Html::span(
                    &["name", "space-before"],
                    &fun.get_name().unwrap_or("_anon".into()),
                ))
                .with_child(
                    HtmlElement::new("span").with_class("params").with_children(
                        fun.get_arguments()
                            .map(|args| {
                                args.iter()
                                    .map(|arg| fmt_param(arg, builder))
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or(Vec::new())
                            .insert_between(|| Html::span(&["comma", "space-after"], ","))
                            .surround(HtmlText::new("(").into(), HtmlText::new(")").into()),
                    ),
                )
                .with_child_opt(
                    fun.is_const_method()
                        .then_some(Html::span(&["keyword", "space-before"], "const")),
                )
                .with_child_opt(
                    fun.is_pure_virtual_method().then_some::<Html>(
                        HtmlList::new(vec![
                            Html::span(&["space-before"], "="),
                            Html::span(&["space-before", "literal"], "0"),
                        ])
                        .into(),
                    ),
                ),
        )
        .with_child(
            HtmlElement::new("div").with_child(
                fun.get_comment()
                    .map(|s| JSDocComment::parse(s, builder).to_html(true))
                    .unwrap_or(Html::p("No description provided")),
            ),
        )
        .into()
}

pub fn fmt_section(title: &str, data: Vec<Html>) -> Html {
    HtmlElement::new("details")
        .with_attr("open", "")
        .with_class("section")
        .with_child(
            HtmlElement::new("summary").with_child(
                HtmlElement::new("span")
                    .with_child(Html::feather("chevron-right"))
                    .with_child(HtmlText::new(title))
                    .with_child(Html::span(&["badge"], &data.len().to_string())),
            ),
        )
        .with_child(HtmlElement::new("div").with_child(HtmlList::new(data)))
        .into()
}

pub fn fmt_header_link(entity: &Entity, config: Arc<Config>) -> Html {
    if let Some(link) = entity.github_url(config.clone()) &&
        let Some(path) = entity.include_path(config.clone()) &&
        let Some(src) = entity.config_source(config)
    {
        let disabled = !src.exists_online;
        HtmlElement::new("a")
            .with_attr_opt("href", (!disabled).then_some(link))
            .with_class("header-link")
            .with_class_opt(disabled.then_some("disabled"))
            .with_child(HtmlElement::new("code")
                .with_class("header-link")
                .with_children(vec![
                    Html::span(&["keyword"], "#include ").into(),
                    Html::span(&["url"], &format!("&lt;{}&gt;", path.to_raw_string()))
                ])
            )
            .into()
    }
    else {
        Html::p("&lt;Not available online&gt;")
    }
}

pub fn output_entity<'e, T: ASTEntry<'e>>(
    entry: &T,
    builder: &Builder,
) -> Vec<(&'static str, Html)> {
    vec![
        ("name", HtmlText::new(entry.name()).into()),
        (
            "description",
            entry
                .entity()
                .get_comment()
                .map(|s| JSDocComment::parse(s, builder).to_html(false))
                .unwrap_or(Html::p("No Description Provided")),
        ),
        ("header_link", fmt_header_link(entry.entity(), builder.config.clone())),
        (
            "examples",
            fmt_section(
                "Examples", 
                entry
                    .entity()
                    .get_comment()
                    .map(|s| JSDocComment::parse(s, builder)
                        .examples()
                        .iter()
                        .map(|example| example.to_html())
                        .collect()
                    )
                    .unwrap_or(Vec::new()),
            )
        ),
    ]
}

pub fn output_classlike<'e, T: ASTEntry<'e>>(
    entry: &T,
    builder: &Builder,
) -> Vec<(&'static str, Html)> {
    let mut ent = output_entity(entry, builder);
    ent.extend(vec![
        (
            "public_static_functions",
            fmt_section(
                "Public static methods",
                entry
                    .entity()
                    .get_children()
                    .iter()
                    .filter(|child| {
                        child.get_kind() == EntityKind::Method
                            && child.is_static_method()
                            && child.get_accessibility() == Some(Accessibility::Public)
                    })
                    .map(|e| fmt_fun_decl(e, builder))
                    .collect::<Vec<_>>(),
            ),
        ),
        (
            "public_member_functions",
            fmt_section(
                "Public member functions",
                entry
                    .entity()
                    .get_children()
                    .iter()
                    .filter(|child| {
                        child.get_kind() == EntityKind::Method
                            && !child.is_static_method()
                            && child.get_accessibility() == Some(Accessibility::Public)
                    })
                    .map(|e| fmt_fun_decl(e, builder))
                    .collect::<Vec<_>>(),
            ),
        ),
        (
            // todo: hide if final class
            "protected_member_functions",
            fmt_section(
                "Protected member functions",
                entry
                    .entity()
                    .get_children()
                    .iter()
                    .filter(|child| {
                        child.get_kind() == EntityKind::Method
                            && !child.is_static_method()
                            && child.get_accessibility() == Some(Accessibility::Protected)
                    })
                    .map(|e| fmt_fun_decl(e, builder))
                    .collect::<Vec<_>>(),
            ),
        ),
        (
            "public_members",
            fmt_section(
                "Fields",
                entry
                    .entity()
                    .get_children()
                    .iter()
                    .filter(|child| {
                        child.get_kind() == EntityKind::FieldDecl
                            && child.get_accessibility() == Some(Accessibility::Public)
                    })
                    .map(|e| fmt_field(e, builder))
                    .collect::<Vec<_>>(),
            ),
        ),
        (
            "protected_members",
            fmt_section(
                "Protected fields",
                entry
                    .entity()
                    .get_children()
                    .iter()
                    .filter(|child| {
                        child.get_kind() == EntityKind::FieldDecl
                            && child.get_accessibility() == Some(Accessibility::Protected)
                    })
                    .map(|e| fmt_field(e, builder))
                    .collect::<Vec<_>>(),
            ),
        ),
    ]);
    ent
}

enum Word {
    Unmatched(String),
    Matched(String),
}

fn fmt_autolinks_recursive(entity: &CppItem, config: Arc<Config>, words: &mut Vec<Word>) {
    for word in words.iter_mut() {
        if let Word::Unmatched(name) = word {
            if *name == entity.name() {
                *word = Word::Matched(format!(
                    "[{name}]({})",
                    entity.entity().docs_url(config.clone())
                ));
            }
        }
    }

    match entity {
        CppItem::Namespace(ns) => {
            for v in ns.entries.values() {
                fmt_autolinks_recursive(v, config.clone(), words);
            }
        },
        _ => {},
    }
}

pub fn fmt_autolinks(builder: &Builder, text: &String) -> String {
    let mut words = text
        // hacky fix to preserve newlines
        .replace("\n", " <<br>> ")
        .split(|c: char| c.is_whitespace())
        .filter(|s| s.len() > 0)
        .map(|w| Word::Unmatched(w.into()))
        .collect();

    for entry in builder.root.entries.values() { 
        fmt_autolinks_recursive(entry, builder.config.clone(), &mut words);
    }

    words.into_iter().map(|word| match word {
        Word::Matched(m) => m,
        Word::Unmatched(w) => w,
    }).collect::<Vec<_>>().join(" ").replace("<<br>>", "\n")
}

fn fmt_emoji(text: &CowStr) -> String {
    fn eat_emoji<'e>(iter: &mut MultiPeek<Chars>) -> Option<&'e str> {
        let mut buffer = String::new();
        let mut i = 0;
        while let Some(d) = iter.peek_nth(i) {
            if d.is_alphanumeric() || *d == '_' {
                buffer.push(*d);
            }
            else if *d == ':' {
                break;
            }
            else {
                return None;
            }
            i += 1;
        }
        if let Some(emoji) = emojis::get_by_shortcode(&buffer) {
            drop(iter.advance_by(i + 1));
            Some(emoji.as_str())
        }
        else {
            None
        }
    }

    let mut res = String::new();
    res.reserve(text.len());

    let mut iter = text.chars().multipeek();
    while let Some(c) = iter.next() {
        if c == ':' && let Some(emoji) = eat_emoji(&mut iter) {
            res.push_str(emoji);
        }
        else {
            res.push(c);
        }
    }

    res
}

pub fn fmt_markdown(text: &String) -> Html {
    let parser = pulldown_cmark::Parser::new_ext(
        &text,
        pulldown_cmark::Options::all()
    );

    let mut content = String::new();
    pulldown_cmark::html::push_html(&mut content, parser.map(|event| match event {
        Event::Text(mut t) => Event::Text(CowStr::Boxed(Box::from(fmt_emoji(&mut t).as_str()))),
        _ => event,
    }));

    HtmlElement::new("div")
        .with_class("text")
        .with_child(Html::Raw(content))
        .into()
}

pub fn extract_title_from_md(text: &String) -> Option<String> {
    let mut parser = pulldown_cmark::Parser::new_ext(
        text, pulldown_cmark::Options::all()
    );

    let name = parser.next()?;
    let Event::Start(tag) = name else { return None };
    let Tag::Heading(_, _, _) = tag else { return None };

    let mut res = String::new();

    while match parser.next() {
        Some(ev) => match ev {
            Event::End(tag) => !matches!(tag, Tag::Heading(_, _, _)),
            Event::Text(text) => {
                res.push_str(&text);
                true
            },
            _ => true,
        },
        None => false,
    } {}

    (res.len() > 0).then_some(res)
}
