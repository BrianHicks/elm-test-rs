#![warn(clippy::pedantic)]

use std::{
    ops::Range,
    path::{Path, PathBuf},
};

use thiserror::Error;
use tree_sitter::Tree;

#[derive(Error, Debug)]
pub enum ExplicitExposedValuesError<'a> {
    #[error("unexpected node kind")]
    UnexpectedNode(tree_sitter::Node<'a>),
    #[error("node should have had a next sibling")]
    ShouldHaveSibling(tree_sitter::Node<'a>),
    #[error("node should have children")]
    ShouldHaveChildren(tree_sitter::Node<'a>),
}

///
/// # Errors
///
/// If the elm file is not valid (it will fail `elm make`).
///
pub fn get_all_exposed_values<'a>(
    tree: &'a Tree,
    source: &'a str,
) -> Result<Vec<&'a str>, ExplicitExposedValuesError<'a>> {
    get_explicit_exposed_values(tree, source)
        .transpose()
        .unwrap_or_else(|| get_all_top_level_values(tree, source))
}

/// `OK(None)` means the file has `exposing(..)` in it and it therefore exposes
/// all top level values.
fn get_explicit_exposed_values<'a>(
    tree: &'a Tree,
    source: &'a str,
) -> Result<Option<Vec<&'a str>>, ExplicitExposedValuesError<'a>> {
    let mut cursor = tree.walk();
    child(&mut cursor)?;
    while cursor.node().kind() != "module_declaration" {
        next_sibling(&mut cursor)?;
    }
    child(&mut cursor)?;
    while cursor.node().kind() != "exposing_list" {
        next_sibling(&mut cursor)?;
    }
    child(&mut cursor)?;
    check_kind(cursor.node(), "exposing")?;
    next_sibling(&mut cursor)?;
    check_kind(cursor.node(), "left_parenthesis")?;
    next_sibling(&mut cursor)?;

    let ret = if cursor.node().kind() == "double_dot" {
        next_sibling(&mut cursor)?;
        None
    } else {
        let mut v = vec![];
        while match cursor.node().kind() {
            "exposed_type" | "comma" | "block_comment" | "line_comment" => true,
            "exposed_value" => {
                let c = ChildCursor::new(&mut cursor)?;
                check_kind(c.child().node(), "lower_case_identifier")?;
                v.push(&source[c.child().node().byte_range()]);
                true
            }
            _ => false,
        } {
            next_sibling(&mut cursor)?;
        }
        Some(v)
    };

    check_kind(cursor.node(), "right_parenthesis")?;
    Ok(ret)
}

/// Gets all top level values from an elm file.
fn get_all_top_level_values<'a>(
    tree: &'a Tree,
    source: &'a str,
) -> Result<Vec<&'a str>, ExplicitExposedValuesError<'a>> {
    let mut cursor = tree.walk();
    child(&mut cursor)?;
    let mut v = vec![];
    loop {
        if cursor.node().kind() == "value_declaration" {
            let mut c1 = ChildCursor::new(&mut cursor)?;
            let c2 = ChildCursor::new(c1.child_mut())?;
            v.push(&source[c2.child().node().byte_range()]);
        }
        if next_sibling(&mut cursor).is_err() {
            break Ok(v);
        }
    }
}

pub struct TestModule {
    pub path: PathBuf,
    pub tests: Vec<String>,
}

/// Find all possible tests (all values) in `test_files`.
pub fn all_tests(
    test_sources: impl IntoIterator<Item = (impl AsRef<Path>, impl AsRef<str>)>,
) -> Result<Vec<TestModule>, String> {
    test_sources
        .into_iter()
        .map(|(file_path, source)| {
            let tree = {
                let mut parser = tree_sitter::Parser::new();
                let language = tree_sitter_elm::language();
                parser.set_language(language).unwrap();
                parser.parse(source.as_ref(), None).unwrap()
            };

            // get_all_exposed_values(&tree, source.as_ref())
            //     .map(|tests| TestModule {
            //         path: file_path.as_ref().to_owned(),
            //         tests: tests.into_iter().map(ToString::to_string).collect(),
            //     })
            //     .map_err(|s| s.to_string())
            let potential_tests = get_all_exposed_values_query(&tree, source.as_ref());
            Ok(TestModule {
                path: file_path.as_ref().to_owned(),
                tests: potential_tests
                    .into_iter()
                    .map(ToString::to_string)
                    .collect(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {

    use tree_sitter::{Parser, Tree};

    fn tree_from_elm(source_code: &str) -> Tree {
        let mut parser = Parser::new();
        let language = tree_sitter_elm::language();
        parser.set_language(language).unwrap();
        parser.parse(source_code, None).unwrap()
    }
    #[test]
    fn smoke() {
        let source_code = "test : Test.Test";
        let tree = tree_from_elm(source_code);
        let root_node = tree.root_node();

        assert_eq!(root_node.kind(), "file");
        assert_eq!(root_node.start_position().column, 0);
        assert_eq!(root_node.end_position().column, 16);
    }
    #[test]
    fn get_explicit_exposed_values() {
        let helper = |source: &str, expected: &Option<Vec<&str>>| {
            let tree = tree_from_elm(source);
            assert_eq!(
                super::get_explicit_exposed_values(&tree, source)
                    .unwrap()
                    .as_ref(),
                expected.as_ref()
            );
        };

        helper("module Main exposing (..)", &None);
        helper("module Main.Pain exposing (..)", &None);
        helper("port module Main.Pain exposing (Int)", &Some(vec![]));
        helper("port module Main.Pain exposing (int)", &Some(vec!["int"]));
        helper(
            "port module Main.Pain exposing (int, Int, test, Test)",
            &Some(vec!["int", "test"]),
        );
        helper(
            "port module Main.Pain exposing (int, Int, {- -}test, Test)",
            &Some(vec!["int", "test"]),
        );
        helper(
            "port module Main.Pain exposing (int, Int, -- comment
    test, Test)",
            &Some(vec!["int", "test"]),
        );
        helper(
            "port module Main.Pain exposing (int, Int,
    test, Test)",
            &Some(vec!["int", "test"]),
        );
        helper(
            "-- some comment
module Main.Pain exposing (int, Int,
    test, Test)",
            &Some(vec!["int", "test"]),
        );
        helper(
            r#"
module{--}Main {-
    {{-}-}-
-}exposing--{-
    ({--}one{--}
    ,
    -- notExport
    two{-{-{-{--}-}{--}-}{-{--}-}-},Type{--}({--}..{--}){--}
    ,    three
    )--
"#,
            &Some(vec!["one", "two", "three"]),
        );
    }
    #[test]
    fn get_all_top_level_values() {
        let helper = |source: &str, expected: &Vec<&str>| {
            let tree = tree_from_elm(source);
            assert_eq!(
                &super::get_all_top_level_values(&tree, source).unwrap(),
                expected
            );
        };

        helper("type Test = Thi", &vec![]);
        helper(
            "test = 3
differentTest: Test.Test
differentTest =
    w
",
            &vec!["test", "differentTest"],
        );
        helper(
            "
type Test = Igore

withNestedValues: Test.Test
withNestedValues =
    let
        shouldIgnore = Test.test
    in
    ()
",
            &vec!["withNestedValues"],
        );
        helper(
            "
type Test = Igore

withNestedValues: Test.Test
withNestedValues a =
    let
        shouldIgnore = Test.test
    in
    ()
",
            &vec!["withNestedValues"],
        );
        helper(
            r#"
module Main exposing ( ..)

one="\"{-"
two="""-}
notAThing = something
\"""
notAThing2 = something
"""
three = '"' {- "
notAThing3 = something
-}
four{--}=--{-
    1
five = something
--}
"#,
            &vec!["one", "two", "three", "four", "five"],
        );
    }
}

fn check_kind<'b>(
    node: tree_sitter::Node<'b>,
    expected: &'static str,
) -> Result<(), ExplicitExposedValuesError<'b>> {
    if node.kind() == expected {
        Ok(())
    } else {
        Err(ExplicitExposedValuesError::UnexpectedNode(node))
    }
}

fn skip_comments<'b>(
    cursor: &mut tree_sitter::TreeCursor<'b>,
) -> Result<(), ExplicitExposedValuesError<'b>> {
    while cursor.node().kind() == "line_comment" {
        if cursor.goto_next_sibling() {
        } else {
            return Err(ExplicitExposedValuesError::ShouldHaveSibling(cursor.node()));
        }
    }
    Ok(())
}

fn child<'b>(
    cursor: &mut tree_sitter::TreeCursor<'b>,
) -> Result<(), ExplicitExposedValuesError<'b>> {
    if cursor.goto_first_child() {
        skip_comments(cursor)
    } else {
        Err(ExplicitExposedValuesError::ShouldHaveChildren(
            cursor.node(),
        ))
    }
}
fn next_sibling<'b>(
    cursor: &mut tree_sitter::TreeCursor<'b>,
) -> Result<(), ExplicitExposedValuesError<'b>> {
    if cursor.goto_next_sibling() {
        skip_comments(cursor)
    } else {
        Err(ExplicitExposedValuesError::ShouldHaveSibling(cursor.node()))
    }
}

/// RAII wrapper around a cursor that provides access to its child.
struct ChildCursor<'a, 'b>(&'a mut tree_sitter::TreeCursor<'b>);

impl<'a, 'b> ChildCursor<'a, 'b> {
    fn new(c: &'a mut tree_sitter::TreeCursor<'b>) -> Result<Self, ExplicitExposedValuesError<'b>> {
        child(c)?;
        Ok(ChildCursor(c))
    }
    fn child(&self) -> &tree_sitter::TreeCursor<'b> {
        self.0
    }
    fn child_mut(&mut self) -> &mut tree_sitter::TreeCursor<'b> {
        self.0
    }
}

impl Drop for ChildCursor<'_, '_> {
    fn drop(&mut self) {
        assert!(self.0.goto_parent());
    }
}

// Query approach ##############################################################

use tree_sitter::Query;

lazy_static::lazy_static! {
    static ref EXPOSING_LIST_QUERY: Query = {
        let query_str = "(module_declaration exposing: (exposing_list) @list)";
        Query::new(tree_sitter_elm::language(), query_str).unwrap()
    };
    static ref DOUBLE_DOT_QUERY: Query = {
        let query_str = "((left_parenthesis) . (double_dot))";
        Query::new(tree_sitter_elm::language(), query_str).unwrap()
    };
    static ref EXPOSED_VALUE_QUERY: Query = {
        let query_str = "(exposed_value) @val";
        Query::new(tree_sitter_elm::language(), query_str).unwrap()
    };
    static ref TOP_LEVEL_VALUE_QUERY: Query = {
        let query_str = "(file (value_declaration . (_ . (_) @name)))";
        Query::new(tree_sitter_elm::language(), query_str).unwrap()
    };
}

fn get_all_exposed_values_query<'a>(tree: &'a Tree, source: &'a str) -> Vec<&'a str> {
    match get_exposing_src_range(tree) {
        None => Vec::new(),
        Some(range) => get_explicit_exposed_values_query(tree, source, range)
            .unwrap_or_else(|| get_all_top_level_values_query(tree, source)),
    }
}

fn get_exposing_src_range<'a>(tree: &'a Tree) -> Option<Range<usize>> {
    tree_sitter::QueryCursor::new()
        .matches(&EXPOSING_LIST_QUERY, tree.root_node(), |_| &[])
        .next()
        .map(|m| m.captures[0].node.byte_range())
}

fn get_explicit_exposed_values_query<'a>(
    tree: &'a Tree,
    source: &'a str,
    range: Range<usize>,
) -> Option<Vec<&'a str>> {
    // Restrict the query cursor search to the exposing list
    let mut query_cursor = tree_sitter::QueryCursor::new();
    query_cursor.set_byte_range(range.start, range.end);

    // Check if we have a "exposing (..)"
    if query_cursor
        .matches(&DOUBLE_DOT_QUERY, tree.root_node(), |_| &[])
        .next()
        .is_some()
    {
        return None;
    }

    // Retrieve all exposed values
    Some(
        query_cursor
            .matches(&EXPOSED_VALUE_QUERY, tree.root_node(), |_| &[])
            .map(|m| &source[m.captures[0].node.byte_range()])
            .collect(),
    )
}

fn get_all_top_level_values_query<'a>(tree: &'a Tree, source: &'a str) -> Vec<&'a str> {
    tree_sitter::QueryCursor::new()
        .matches(&TOP_LEVEL_VALUE_QUERY, tree.root_node(), |_| &[])
        .map(|m| &source[m.captures[0].node.byte_range()])
        .collect()
}
