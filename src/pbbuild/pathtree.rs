use std::{
    collections::{BTreeMap, HashSet},
    fmt::Debug,
    rc::Rc,
};

use super::{build::PbBuild, path::VirtPath, value::Value};
use crate::{
    Expr,
    pbexpr::{Error, ErrorType, Exportable, ExprSet, ExprType, Loc, Referrable, Result},
    strkey::StrKey,
};

/// A tree of lists and objects with a path at every leaf.
///
/// Used for the `input`, `output` and `deps` arguments of a build, which may
/// be any nesting of lists and objects of paths.
#[derive(Debug, Clone)]
pub enum PathTree {
    /// Always a `Value::Path`
    Leaf(Value),
    List(Vec<PathTree>),
    Object(BTreeMap<StrKey, PathTree>),
}

impl PathTree {
    /// Resolve `expr` recursively and build the tree.
    ///
    /// Every leaf must be a path, and every other node a list or an object.
    pub fn from_expr<F: Clone + Debug + Referrable>(
        expr: &Expr<Value, F>,
        field_name: &str,
    ) -> Result<Self, F> {
        expr.resolve().map_err(|e| e.reref(&expr.get_loc()))?;
        match &expr.inner_ref().tok {
            ExprType::Value(value) if value.is_path() => Ok(PathTree::Leaf(value.clone())),
            ExprType::Value(_) => Err(Error::new(
                ErrorType::Type,
                format!(
                    "incompatible type in build arg '{}', got {}",
                    field_name,
                    expr.diag()
                ),
            )
            .reref(&expr.get_loc())),
            ExprType::List(exprs) => Ok(PathTree::List(
                exprs
                    .iter()
                    .map(|e| PathTree::from_expr(e, field_name))
                    .collect::<Result<_, F>>()?,
            )),
            ExprType::Object(fields) => Ok(PathTree::Object(
                fields
                    .iter()
                    .map(|(name, e)| Ok((*name, PathTree::from_expr(e, field_name)?)))
                    .collect::<Result<_, F>>()?,
            )),
            _ => Err(Error::new(
                ErrorType::Type,
                format!(
                    "build arg '{}' must be a path, or lists and objects of paths, got {}",
                    field_name,
                    expr.diag()
                ),
            )
            .reref(&expr.get_loc())),
        }
    }

    /// Call `f` for every leaf, depth first: objects in key order, lists in
    /// index order.
    fn for_each_leaf<'a>(&'a self, f: &mut impl FnMut(&'a VirtPath, &'a Vec<Rc<PbBuild>>)) {
        match self {
            PathTree::Leaf(Value::Path { path, depends }) => f(path, depends),
            PathTree::Leaf(_) => unreachable!("PathTree leaf is always a path"),
            PathTree::List(items) => items.iter().for_each(|item| item.for_each_leaf(f)),
            PathTree::Object(fields) => fields.values().for_each(|item| item.for_each_leaf(f)),
        }
    }

    /// All leaf paths, depth first, duplicates included.
    pub fn paths(&self) -> Vec<VirtPath> {
        let mut paths = Vec::new();
        self.for_each_leaf(&mut |path, _| paths.push(path.clone()));
        paths
    }

    /// All leaf paths, depth first, with duplicates removed. The first
    /// occurrence of each path is kept.
    pub fn unique_paths(&self) -> Vec<VirtPath> {
        let mut seen = HashSet::new();
        let mut paths = Vec::new();
        self.for_each_leaf(&mut |path, _| {
            if seen.insert(path) {
                paths.push(path.clone());
            }
        });
        paths
    }

    /// The builds that the leaf paths depend on.
    pub fn depends(&self) -> Vec<Rc<PbBuild>> {
        let mut builds = Vec::new();
        self.for_each_leaf(&mut |_, depends| builds.extend(depends.iter().cloned()));
        builds
    }

    /// Rebuild the same shape as an expression tree, with every leaf being a
    /// path that depends on `build`. Any earlier dependencies of the leaves
    /// are dropped.
    pub fn to_expr<F: Clone + Debug + Referrable>(
        &self,
        build: &Rc<PbBuild>,
        loc: &Option<Loc<F>>,
    ) -> Expr<Value, F> {
        match self {
            PathTree::Leaf(Value::Path { path, .. }) => ExprType::Value(Value::Path {
                path: path.clone(),
                depends: vec![build.clone()],
            }),
            PathTree::Leaf(_) => unreachable!("PathTree leaf is always a path"),
            PathTree::List(items) => {
                ExprType::List(items.iter().map(|item| item.to_expr(build, loc)).collect())
            }
            PathTree::Object(fields) => ExprType::Object(
                fields
                    .iter()
                    .map(|(name, item)| (*name, item.to_expr(build, loc)))
                    .collect::<ExprSet<Value, F>>(),
            ),
        }
        .reref(loc.clone())
    }
}
