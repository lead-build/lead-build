use std::collections::BTreeMap;
use std::fmt::{Debug, Display};

use crate::lang::Referrable;
use crate::{
    Expr,
    lang::{Error, ErrorType, Exportable, ExprBuiltin, ExprOps, ExprSet, ExprType, Result},
    strkey::StrKey,
};

#[derive(PartialEq)]
enum CompoundType {
    Tuple,
    List,
    Object,
}

type IMap<T> = BTreeMap<usize, T>;

impl CompoundType {
    pub fn decompose<T, F>(arg: Expr<T, F>) -> Result<(Self, IMap<Expr<T, F>>), F>
    where
        T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
        F: Clone + Debug + Referrable,
    {
        arg.resolve()?;
        match &arg.inner_ref().tok {
            ExprType::List(items) => Ok((
                CompoundType::List,
                items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| (i, item.clone()))
                    .collect(),
            )),
            ExprType::Tuple(items) => Ok((
                CompoundType::Tuple,
                items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| (i, item.clone()))
                    .collect(),
            )),
            ExprType::Object(fields) => Ok((
                CompoundType::Object,
                fields
                    .iter()
                    .map(|(key, value)| (key.get_raw_id(), value.clone()))
                    .collect(),
            )),
            _ => Err(Error::new(
                ErrorType::Type,
                format!("expected tuple, list, or object, got {}", arg),
            )
            .reref(&arg.get_loc())),
        }
    }

    pub fn recompose<T, F>(&self, elements: IMap<Expr<T, F>>) -> Expr<T, F>
    where
        T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
        F: Clone + Debug + Referrable,
    {
        match self {
            CompoundType::List => ExprType::List(Self::pad_indexed(elements)).reref(None),
            CompoundType::Tuple => ExprType::Tuple(Self::pad_indexed(elements)).reref(None),
            CompoundType::Object => ExprType::Object(
                elements
                    .into_iter()
                    .map(|(id, value)| (StrKey::from_raw_id(id), value))
                    .collect::<ExprSet<T, F>>(),
            )
            .reref(None),
        }
    }

    /* Fills any gaps in the index with Null, so the resulting list/tuple keeps the original indices */
    fn pad_indexed<T, F>(elements: IMap<Expr<T, F>>) -> Vec<Expr<T, F>>
    where
        T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
        F: Clone + Debug + Referrable,
    {
        let len = elements.keys().next_back().map_or(0, |&k| k + 1);
        let mut items: Vec<Expr<T, F>> = (0..len).map(|_| ExprType::Null.reref(None)).collect();
        for (idx, expr) in elements {
            items[idx] = expr;
        }
        items
    }
}

fn btree_transpose<K: Ord + Clone, T>(
    map: BTreeMap<K, BTreeMap<K, T>>,
) -> BTreeMap<K, BTreeMap<K, T>> {
    let mut transposed: BTreeMap<K, BTreeMap<K, T>> = BTreeMap::new();
    for (outer_idx, inner_map) in map {
        for (inner_idx, value) in inner_map {
            transposed
                .entry(inner_idx)
                .or_default()
                .insert(outer_idx.clone(), value);
        }
    }
    transposed
}

#[derive(Debug)]
pub struct BuiltinOpsTranspose;

impl<T, F> ExprBuiltin<T, F> for BuiltinOpsTranspose
where
    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
    F: Clone + Debug + Referrable,
{
    fn get_name(&self) -> StrKey {
        "zip".into()
    }

    fn call(&self, arg: Expr<T, F>) -> Result<Expr<T, F>, F> {
        let loc = arg.get_loc();
        let (outer_type, outer_elems) = CompoundType::decompose(arg)?;

        let mut inner_type: Option<CompoundType> = None;
        let mut elems: IMap<IMap<Expr<T, F>>> = IMap::new();

        for (idx, expr) in outer_elems {
            let elem_loc = expr.get_loc();
            let (elem_type, elem_elems) = CompoundType::decompose(expr)?;
            match &inner_type {
                None => inner_type = Some(elem_type),
                Some(expected) if *expected == elem_type => {}
                Some(_) => {
                    return Err(Error::new(
                        ErrorType::Type,
                        "zip elements must all be of the same compound type",
                    )
                    .reref(&elem_loc));
                }
            }
            elems.insert(idx, elem_elems);
        }

        let inner_type = inner_type.ok_or_else(|| {
            Error::new(ErrorType::Type, "zip requires at least one element").reref(&loc)
        })?;

        let transposed = btree_transpose(elems);

        let out = inner_type.recompose(
            transposed
                .into_iter()
                .map(|(idx, transposed_inner_map)| {
                    (idx, outer_type.recompose(transposed_inner_map))
                })
                .collect::<IMap<Expr<T, F>>>(),
        );
        Ok(out)
    }
}

#[derive(Debug)]
pub struct BuiltinOpsTransposeObjs;

impl<T, F> ExprBuiltin<T, F> for BuiltinOpsTransposeObjs
where
    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
    F: Clone + Debug + Referrable,
{
    fn get_name(&self) -> StrKey {
        "transposeObjs".into()
    }

    fn call(&self, arg: Expr<T, F>) -> Result<Expr<T, F>, F> {
        let loc = arg.get_loc();

        arg.resolve()?;
        let binding = arg.inner_ref();
        let elems = match &binding.tok {
            ExprType::List(items) => Ok(items),
            _ => Err(
                Error::new(ErrorType::Type, format!("expected list {}", arg)).reref(&arg.get_loc()),
            ),
        }?;

        let mut new_args: BTreeMap<StrKey, Vec<Expr<T, F>>> = BTreeMap::new();
        for inner_expr in elems.iter() {
            inner_expr.resolve()?;
            let binding = inner_expr.inner_ref();
            let inner_obj = match &binding.tok {
                ExprType::Object(items) => Ok(items),
                _ => Err(Error::new(
                    ErrorType::Type,
                    format!("expected objects, got {}", inner_expr),
                )
                .reref(&inner_expr.get_loc())),
            }?;

            for (key, value) in inner_obj.iter() {
                new_args.entry(*key).or_default().push(value.clone());
            }
        }

        let new_obj = new_args
            .into_iter()
            .map(|(key, values)| (key, ExprType::List(values).reref(loc.clone())))
            .collect::<BTreeMap<StrKey, Expr<T, F>>>();

        Ok(ExprType::Object(new_obj).reref(loc))
    }
}

pub fn get_ops_builtins<T, F>() -> Result<Expr<T, F>, F>
where
    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
    F: Clone + Debug + Referrable,
{
    let mut opsset = ExprSet::new();
    opsset.insert(StrKey::from("transpose"), Expr::new_builtin(BuiltinOpsTranspose));
    opsset.insert(
        StrKey::from("transposeObjs"),
        Expr::new_builtin(BuiltinOpsTransposeObjs),
    );
    Ok(ExprType::Object(opsset).builtin())
}
