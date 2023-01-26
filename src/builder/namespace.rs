use std::collections::HashMap;

use clang::{Entity, EntityKind};

use crate::url::UrlPath;

use super::{
    builder::{ASTEntry, BuildResult, Builder, EntityMethods, Entry, NavItem},
    class::Class,
    function::Function,
    struct_::Struct,
};

pub enum CppItem<'e> {
    Namespace(Namespace<'e>),
    Class(Class<'e>),
    Struct(Struct<'e>),
    Function(Function<'e>),
}

impl<'e> CppItem<'e> {
    fn get(&'e self, matcher: &dyn Fn(&dyn ASTEntry<'e>) -> bool, out: &mut Vec<&'e dyn ASTEntry<'e>>) {
        match self {
            CppItem::Namespace(ns) => {
                if matcher(ns) {
                    out.push(ns);
                }
                for entry in ns.entries.values() {
                    entry.get(&matcher, out);
                }
            },
            CppItem::Class(cls) => {
                if matcher(cls) {
                    out.push(cls);
                }
            },
            CppItem::Struct(cls) => {
                if matcher(cls) {
                    out.push(cls);
                }
            },
            CppItem::Function(fun) => {
                if matcher(fun) {
                    out.push(fun);
                }
            },
        }
    }
}

impl<'e> Entry<'e> for CppItem<'e> {
    fn name(&self) -> String {
        match self {
            CppItem::Namespace(ns) => ns.name(),
            CppItem::Class(cs) => cs.name(),
            CppItem::Struct(st) => st.name(),
            CppItem::Function(st) => st.name(),
        }
    }

    fn url(&self) -> UrlPath {
        match self {
            CppItem::Namespace(ns) => ns.url(),
            CppItem::Class(cs) => cs.url(),
            CppItem::Struct(st) => st.url(),
            CppItem::Function(st) => st.url(),
        }
    }

    fn build(&self, builder: &Builder<'e>) -> BuildResult {
        match self {
            CppItem::Namespace(ns) => ns.build(builder),
            CppItem::Class(cs) => cs.build(builder),
            CppItem::Struct(st) => st.build(builder),
            CppItem::Function(st) => st.build(builder),
        }
    }

    fn nav(&self) -> NavItem {
        match self {
            CppItem::Namespace(ns) => ns.nav(),
            CppItem::Class(cs) => cs.nav(),
            CppItem::Struct(st) => st.nav(),
            CppItem::Function(st) => st.nav(),
        }
    }
}

impl<'e> ASTEntry<'e> for CppItem<'e> {
    fn entity(&self) -> &Entity<'e> {
        match self {
            CppItem::Class(c) => c.entity(),
            CppItem::Function(c) => c.entity(),
            CppItem::Namespace(c) => c.entity(),
            CppItem::Struct(c) => c.entity(),
        }
    }
}

pub struct Namespace<'e> {
    entity: Entity<'e>,
    pub entries: HashMap<String, CppItem<'e>>,
}

impl<'e> Namespace<'e> {
    pub fn new(entity: Entity<'e>) -> Self {
        let mut ret = Self {
            entity,
            entries: HashMap::new(),
        };
        ret.load_entries();
        ret
    }

    fn load_entries(&mut self) {
        for child in &self.entity.get_children() {
            if child.is_in_system_header() || child.get_name().is_none() {
                continue;
            }
            match child.get_kind() {
                EntityKind::Namespace => {
                    let entry = Namespace::new(*child);
                    // Merge existing entries of namespace
                    if let Some(key) = self.entries.get_mut(&entry.name()) {
                        if let CppItem::Namespace(ns) = key {
                            ns.entries.extend(entry.entries);
                        }
                    }
                    // Insert new namespace
                    else {
                        self.entries.insert(entry.name(), CppItem::Namespace(entry));
                    }
                }

                EntityKind::StructDecl => {
                    if child.is_definition() {
                        let entry = Struct::new(*child);
                        self.entries.insert(entry.name(), CppItem::Struct(entry));
                    }
                }

                EntityKind::ClassDecl | EntityKind::ClassTemplate => {
                    if child.is_definition() {
                        let entry = Class::new(*child);
                        self.entries.insert(entry.name(), CppItem::Class(entry));
                    }
                }

                EntityKind::FunctionDecl => {
                    let entry = Function::new(*child);
                    self.entries.insert(entry.name(), CppItem::Function(entry));
                }

                _ => continue,
            }
        }
    }

    // so apparently if you make this a <M: Fn(&dyn ASTEntry<'e>) -> bool> 
    // rustc crashes
    pub fn get(&'e self, matcher: &dyn Fn(&dyn ASTEntry<'e>) -> bool) -> Vec<&'e dyn ASTEntry<'e>> {
        let mut res = Vec::new();
        for entry in self.entries.values() {
            entry.get(&matcher, &mut res);
        }
        res
    }
}

impl<'e> Entry<'e> for Namespace<'e> {
    fn build(&self, builder: &Builder<'e>) -> BuildResult {
        let mut handles = Vec::new();
        for entry in self.entries.values() {
            handles.extend(entry.build(builder)?);
        }
        Ok(handles)
    }

    fn nav(&self) -> NavItem {
        let mut entries = self.entries.iter().collect::<Vec<_>>();

        // Namespaces first in sorted order, everything else after in sorted order
        entries.sort_by_key(|p| (!matches!(p.1, CppItem::Namespace(_)), p.0));

        if self.entity.get_kind() == EntityKind::TranslationUnit {
            NavItem::new_root(None, entries.iter().map(|e| e.1.nav()).collect())
        } else {
            NavItem::new_dir(
                &self.name(),
                entries.iter().map(|e| e.1.nav()).collect(),
                None,
            )
        }
    }

    fn name(&self) -> String {
        self.entity
            .get_name()
            .unwrap_or("<Anonymous namespace>".into())
    }

    fn url(&self) -> UrlPath {
        UrlPath::new_with_path(self.entity.full_name())
    }
}

impl<'e> ASTEntry<'e> for Namespace<'e> {
    fn entity(&self) -> &Entity<'e> {
        &self.entity
    }
}
