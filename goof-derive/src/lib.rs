//! Derive macro for the `goof` error library.
//!
//! This implementation avoids the `syn` crate entirely, parsing the
//! derive input from raw `proc_macro2` token trees.

use proc_macro::TokenStream;
use proc_macro2::{
    Delimiter, Ident, Literal, Punct, Spacing, Span, TokenStream as TokenStream2, TokenTree,
};
use quote::{format_ident, quote};

// ============================================================================
// Entry point
// ============================================================================

/// Derive macro that generates implementations of
/// [`core::error::Error`], [`core::fmt::Display`], and optionally
/// [`core::convert::From`] following the same attribute API as
/// `thiserror`.
///
/// # Attributes
///
/// ## `#[error("...")]`
///
/// Placed on a struct or on each variant of an enum.  Generates a
/// `Display` implementation using the provided format string.
///
/// Field interpolation shorthands:
/// - `{var}` → `self.var` (or the matching named field)
/// - `{0}` → `self.0` (tuple field)
/// - `{var:?}` → debug-format of `self.var`
///
/// In additional format arguments, refer to fields with a leading dot:
/// `.var` or `.0`.
///
/// ## `#[error(transparent)]`
///
/// Delegates `Display` and `source()` to the single inner field.
///
/// ## `#[source]`
///
/// Marks a field as the error source.  A field literally named
/// `source` is treated as `#[source]` automatically.
///
/// ## `#[from]`
///
/// Generates a `From<T>` impl for the annotated field's type.
/// Implies `#[source]`.
///
/// ## `#[backtrace]`
///
/// Marks a field whose type path ends in `Backtrace` for use in
/// `Error::provide()` (nightly only — the generated code is
/// feature-gated behind `#[cfg(error_generic_member_access)]`).
#[proc_macro_derive(Error, attributes(error, source, from, backtrace))]
pub fn derive_error(input: TokenStream) -> TokenStream {
    match expand(input.into()) {
        Ok(ts) => ts.into(),
        Err(e) => e.into(),
    }
}

// ============================================================================
// Error helper (compile_error! without syn)
// ============================================================================

fn error(span: Span, msg: &str) -> TokenStream2 {
    let msg_lit = Literal::string(msg);
    quote::quote_spanned!(span => compile_error!(#msg_lit);)
}

// ============================================================================
// Lightweight IR parsed from raw token trees
// ============================================================================

/// Top-level information about the derive input.
struct DeriveInput {
    ident: Ident,
    generics: Generics,
    attrs: Vec<Attr>,
    body: Body,
}

/// A parsed field (from a struct or enum variant).
#[derive(Clone)]
struct Field {
    name: Option<Ident>,
    index: usize,
    ty: TokenStream2,
    attrs: Vec<Attr>,
}

/// A parsed attribute: `#[ident]` or `#[ident(...)]` or `#[ident = "..."]`.
#[derive(Clone)]
struct Attr {
    name: String,
    tokens: Option<TokenStream2>,
}

enum Body {
    Struct(FieldSet),
    Enum(Vec<Variant>),
}

struct Variant {
    ident: Ident,
    fields: FieldSet,
    attrs: Vec<Attr>,
}

#[derive(Clone)]
enum FieldSet {
    Named(Vec<Field>),
    Unnamed(Vec<Field>),
    Unit,
}

/// Minimal generics representation.
struct Generics {
    params: TokenStream2,
    where_clause: Option<TokenStream2>,
}

/// The parsed contents of `#[error("...", args...)]`.
struct ErrorMessage {
    fmt_str: Literal,
    fmt_value: String,
    args: Vec<TokenStream2>,
}

type FieldInfo = (Option<Ident>, usize, TokenStream2);

// ============================================================================
// Parsing from raw token trees
// ============================================================================

fn expand(input: TokenStream2) -> Result<TokenStream2, TokenStream2> {
    let parsed = parse_derive_input(input)?;

    let name = &parsed.ident;
    let (impl_generics, ty_generics, where_clause) = split_generics(&parsed.generics);

    match &parsed.body {
        Body::Struct(fields) => expand_struct(
            name,
            &impl_generics,
            &ty_generics,
            &where_clause,
            &parsed.attrs,
            fields,
        ),
        Body::Enum(variants) => {
            expand_enum(name, &impl_generics, &ty_generics, &where_clause, variants)
        }
    }
}

fn parse_derive_input(input: TokenStream2) -> Result<DeriveInput, TokenStream2> {
    let mut tokens = input.into_iter().peekable();

    // Collect outer attributes and skip `pub`, `pub(crate)`, etc.
    let mut attrs = Vec::new();
    loop {
        match tokens.peek() {
            Some(TokenTree::Punct(p)) if p.as_char() == '#' => {
                let _hash = tokens.next();
                // Must be followed by `[...]`
                if let Some(TokenTree::Group(g)) = tokens.next() {
                    if g.delimiter() == Delimiter::Bracket {
                        attrs.push(parse_attr(g.stream()));
                    }
                }
            }
            Some(TokenTree::Ident(id)) if *id == "pub" || *id == "crate" || *id == "super" => {
                let _vis = tokens.next();
                // Eat `(crate)` or `(super)` or `(in path)` if present.
                if let Some(TokenTree::Group(g)) = tokens.peek() {
                    if g.delimiter() == Delimiter::Parenthesis {
                        tokens.next();
                    }
                }
            }
            _ => break,
        }
    }

    // Expect `struct` or `enum`
    let kind = match tokens.next() {
        Some(TokenTree::Ident(id)) => id.to_string(),
        _ => return Err(error(Span::call_site(), "expected `struct` or `enum`")),
    };

    // Name
    let ident = match tokens.next() {
        Some(TokenTree::Ident(id)) => id,
        _ => return Err(error(Span::call_site(), "expected type name")),
    };

    // Generics (everything before the body or semicolon)
    let (generics, body_tokens) = parse_generics_and_body(&mut tokens)?;

    let body = match kind.as_str() {
        "struct" => Body::Struct(parse_fields(body_tokens)?),
        "enum" => Body::Enum(parse_enum_variants(body_tokens)?),
        _ => return Err(error(ident.span(), "derive(Error) does not support unions")),
    };

    Ok(DeriveInput {
        ident,
        generics,
        attrs,
        body,
    })
}

fn parse_attr(tokens: TokenStream2) -> Attr {
    let mut iter = tokens.into_iter();
    let name = match iter.next() {
        Some(TokenTree::Ident(id)) => id.to_string(),
        _ => {
            return Attr {
                name: String::new(),
                tokens: None,
            };
        }
    };
    let rest: TokenStream2 = iter.collect();
    let tokens = if rest.is_empty() {
        None
    } else {
        // If the rest starts with a parenthesized group, unwrap it.
        let mut rest_iter = rest.into_iter().peekable();
        if let Some(TokenTree::Group(g)) = rest_iter.peek() {
            if g.delimiter() == Delimiter::Parenthesis {
                let g = if let Some(TokenTree::Group(g)) = rest_iter.next() {
                    g
                } else {
                    unreachable!()
                };
                Some(g.stream())
            } else {
                let all: TokenStream2 = std::iter::once(TokenTree::Group(g.clone()))
                    .chain(rest_iter)
                    .collect();
                Some(all)
            }
        } else {
            let all: TokenStream2 = rest_iter.collect();
            if all.is_empty() { None } else { Some(all) }
        }
    };
    Attr { name, tokens }
}

fn parse_generics_and_body(
    tokens: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>,
) -> Result<(Generics, TokenStream2), TokenStream2> {
    let mut params = TokenStream2::new();
    let mut where_clause = None;

    // Check for `<...>` generics
    if matches!(tokens.peek(), Some(TokenTree::Punct(p)) if p.as_char() == '<') {
        tokens.next(); // consume `<`
        let mut depth = 1u32;
        let mut gen_tokens = Vec::new();
        for tok in tokens.by_ref() {
            match &tok {
                TokenTree::Punct(p) if p.as_char() == '<' => depth += 1,
                TokenTree::Punct(p) if p.as_char() == '>' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            gen_tokens.push(tok);
        }
        params = gen_tokens.into_iter().collect();
    }

    // Now we might see `where ...` before `{` / `(` / `;`
    let mut where_tokens = Vec::new();
    let mut collecting_where = false;
    let mut body = TokenStream2::new();

    while let Some(tok) = tokens.peek() {
        match tok {
            TokenTree::Ident(id) if *id == "where" => {
                collecting_where = true;
                tokens.next(); // consume `where`
            }
            TokenTree::Group(g)
                if g.delimiter() == Delimiter::Brace || g.delimiter() == Delimiter::Parenthesis =>
            {
                if collecting_where && !where_tokens.is_empty() {
                    where_clause = Some(where_tokens.into_iter().collect());
                }
                if let Some(TokenTree::Group(g)) = tokens.next() {
                    body = g.stream();
                }
                break;
            }
            TokenTree::Punct(p) if p.as_char() == ';' => {
                // Unit struct: `struct Foo;`
                if collecting_where && !where_tokens.is_empty() {
                    where_clause = Some(where_tokens.into_iter().collect());
                }
                tokens.next(); // consume `;`
                break;
            }
            _ => {
                let tok = tokens.next().unwrap();
                if collecting_where {
                    where_tokens.push(tok);
                }
                // else: stray token between generics and body — skip
            }
        }
    }

    Ok((
        Generics {
            params,
            where_clause,
        },
        body,
    ))
}

fn parse_fields(tokens: TokenStream2) -> Result<FieldSet, TokenStream2> {
    if tokens.is_empty() {
        return Ok(FieldSet::Unit);
    }

    // Determine whether these are named or unnamed fields by looking
    // at the token structure.  Named fields: `name: Type, ...`
    // Unnamed fields: `Type, ...` (from tuple struct body)
    //
    // We detect named fields by checking if the pattern `ident :` appears
    // (skipping attributes).
    let token_vec: Vec<TokenTree> = tokens.into_iter().collect();
    let is_named = detect_named_fields(&token_vec);

    if is_named {
        parse_named_fields(token_vec)
    } else {
        parse_unnamed_fields(token_vec)
    }
}

fn detect_named_fields(tokens: &[TokenTree]) -> bool {
    // Skip leading attributes `#[...]`, then look for `ident :`
    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i] {
            TokenTree::Punct(p) if p.as_char() == '#' => {
                i += 1; // skip `#`
                if i < tokens.len() {
                    if let TokenTree::Group(g) = &tokens[i] {
                        if g.delimiter() == Delimiter::Bracket {
                            i += 1; // skip `[...]`
                            continue;
                        }
                    }
                }
            }
            TokenTree::Ident(_) => {
                // Check if the next non-whitespace token is `:`
                if i + 1 < tokens.len() {
                    if let TokenTree::Punct(p) = &tokens[i + 1] {
                        if p.as_char() == ':' && p.spacing() == Spacing::Alone {
                            return true;
                        }
                    }
                }
                return false;
            }
            _ => return false,
        }
    }
    false
}

fn parse_named_fields(tokens: Vec<TokenTree>) -> Result<FieldSet, TokenStream2> {
    let mut fields = Vec::new();
    let mut i = 0;
    let mut idx = 0;

    while i < tokens.len() {
        // Collect field attributes
        let mut field_attrs = Vec::new();
        while i < tokens.len() {
            if let TokenTree::Punct(p) = &tokens[i] {
                if p.as_char() == '#' {
                    i += 1;
                    if i < tokens.len() {
                        if let TokenTree::Group(g) = &tokens[i] {
                            if g.delimiter() == Delimiter::Bracket {
                                field_attrs.push(parse_attr(g.stream()));
                                i += 1;
                                continue;
                            }
                        }
                    }
                }
            }
            break;
        }

        // Skip visibility keywords like `pub`, `pub(crate)`
        if i < tokens.len() {
            if let TokenTree::Ident(id) = &tokens[i] {
                if *id == "pub" {
                    i += 1;
                    if i < tokens.len() {
                        if let TokenTree::Group(g) = &tokens[i] {
                            if g.delimiter() == Delimiter::Parenthesis {
                                i += 1; // skip `(crate)` etc
                            }
                        }
                    }
                }
            }
        }

        if i >= tokens.len() {
            break;
        }

        // Field name
        let name = match &tokens[i] {
            TokenTree::Ident(id) => id.clone(),
            _ => break,
        };
        i += 1;

        // Colon
        if i < tokens.len() {
            if let TokenTree::Punct(p) = &tokens[i] {
                if p.as_char() == ':' {
                    i += 1;
                }
            }
        }

        // Type tokens — collect until `,` or end
        let mut ty_tokens = Vec::new();
        while i < tokens.len() {
            if let TokenTree::Punct(p) = &tokens[i] {
                if p.as_char() == ',' && p.spacing() == Spacing::Alone {
                    i += 1; // consume comma
                    break;
                }
            }
            ty_tokens.push(tokens[i].clone());
            i += 1;
        }

        let ty: TokenStream2 = ty_tokens.into_iter().collect();
        fields.push(Field {
            name: Some(name),
            index: idx,
            ty,
            attrs: field_attrs,
        });
        idx += 1;
    }

    Ok(FieldSet::Named(fields))
}

fn parse_unnamed_fields(tokens: Vec<TokenTree>) -> Result<FieldSet, TokenStream2> {
    let mut fields = Vec::new();
    let mut i = 0;
    let mut idx = 0;

    while i < tokens.len() {
        // Collect field attributes
        let mut field_attrs = Vec::new();
        while i < tokens.len() {
            if let TokenTree::Punct(p) = &tokens[i] {
                if p.as_char() == '#' {
                    i += 1;
                    if i < tokens.len() {
                        if let TokenTree::Group(g) = &tokens[i] {
                            if g.delimiter() == Delimiter::Bracket {
                                field_attrs.push(parse_attr(g.stream()));
                                i += 1;
                                continue;
                            }
                        }
                    }
                }
            }
            break;
        }

        // Skip visibility keywords
        if i < tokens.len() {
            if let TokenTree::Ident(id) = &tokens[i] {
                if *id == "pub" {
                    i += 1;
                    if i < tokens.len() {
                        if let TokenTree::Group(g) = &tokens[i] {
                            if g.delimiter() == Delimiter::Parenthesis {
                                i += 1;
                            }
                        }
                    }
                }
            }
        }

        if i >= tokens.len() {
            break;
        }

        // Type tokens — collect until `,` at depth 0 or end.
        // We need to track `<>` depth because types like
        // `Vec<String>` contain commas in generic args... wait no
        // they don't, but `HashMap<K, V>` does. Track angle brackets.
        let mut ty_tokens = Vec::new();
        let mut angle_depth = 0u32;
        while i < tokens.len() {
            match &tokens[i] {
                TokenTree::Punct(p)
                    if p.as_char() == ',' && angle_depth == 0 && p.spacing() == Spacing::Alone =>
                {
                    i += 1;
                    break;
                }
                TokenTree::Punct(p) if p.as_char() == '<' => {
                    angle_depth += 1;
                    ty_tokens.push(tokens[i].clone());
                    i += 1;
                }
                TokenTree::Punct(p) if p.as_char() == '>' => {
                    angle_depth = angle_depth.saturating_sub(1);
                    ty_tokens.push(tokens[i].clone());
                    i += 1;
                }
                _ => {
                    ty_tokens.push(tokens[i].clone());
                    i += 1;
                }
            }
        }

        if ty_tokens.is_empty() {
            break;
        }

        let ty: TokenStream2 = ty_tokens.into_iter().collect();
        fields.push(Field {
            name: None,
            index: idx,
            ty,
            attrs: field_attrs,
        });
        idx += 1;
    }

    Ok(FieldSet::Unnamed(fields))
}

fn parse_enum_variants(tokens: TokenStream2) -> Result<Vec<Variant>, TokenStream2> {
    let mut variants = Vec::new();
    let token_vec: Vec<TokenTree> = tokens.into_iter().collect();
    let mut i = 0;

    while i < token_vec.len() {
        // Collect variant attributes
        let mut attrs = Vec::new();
        while i < token_vec.len() {
            if let TokenTree::Punct(p) = &token_vec[i] {
                if p.as_char() == '#' {
                    i += 1;
                    if i < token_vec.len() {
                        if let TokenTree::Group(g) = &token_vec[i] {
                            if g.delimiter() == Delimiter::Bracket {
                                attrs.push(parse_attr(g.stream()));
                                i += 1;
                                continue;
                            }
                        }
                    }
                }
            }
            break;
        }

        if i >= token_vec.len() {
            break;
        }

        // Variant name
        let vname = match &token_vec[i] {
            TokenTree::Ident(id) => id.clone(),
            _ => {
                i += 1;
                continue;
            }
        };
        i += 1;

        // Fields: could be `(...)`, `{...}`, or nothing (unit)
        let fields = if i < token_vec.len() {
            match &token_vec[i] {
                TokenTree::Group(g) if g.delimiter() == Delimiter::Parenthesis => {
                    let fs = parse_unnamed_fields(g.stream().into_iter().collect())?;
                    i += 1;
                    fs
                }
                TokenTree::Group(g) if g.delimiter() == Delimiter::Brace => {
                    let fs = parse_named_fields(g.stream().into_iter().collect())?;
                    i += 1;
                    fs
                }
                _ => FieldSet::Unit,
            }
        } else {
            FieldSet::Unit
        };

        // Skip `= discriminant` if present
        if i < token_vec.len() {
            if let TokenTree::Punct(p) = &token_vec[i] {
                if p.as_char() == '=' {
                    i += 1;
                    // skip until `,` or end
                    while i < token_vec.len() {
                        if let TokenTree::Punct(p) = &token_vec[i] {
                            if p.as_char() == ',' {
                                break;
                            }
                        }
                        i += 1;
                    }
                }
            }
        }

        // Skip comma
        if i < token_vec.len() {
            if let TokenTree::Punct(p) = &token_vec[i] {
                if p.as_char() == ',' {
                    i += 1;
                }
            }
        }

        variants.push(Variant {
            ident: vname,
            fields,
            attrs,
        });
    }

    Ok(variants)
}

// ============================================================================
// Generics handling
// ============================================================================

fn split_generics(generics: &Generics) -> (TokenStream2, TokenStream2, TokenStream2) {
    if generics.params.is_empty() {
        let wc = generics
            .where_clause
            .as_ref()
            .map(|w| quote!(where #w))
            .unwrap_or_default();
        return (TokenStream2::new(), TokenStream2::new(), wc);
    }

    let params = &generics.params;

    // For impl generics, we keep everything as-is.
    let impl_generics = quote!(<#params>);

    // For type generics, strip bounds: keep only names and lifetimes.
    let ty_generics_inner = strip_bounds(params.clone());
    let ty_generics = quote!(<#ty_generics_inner>);

    let wc = generics
        .where_clause
        .as_ref()
        .map(|w| quote!(where #w))
        .unwrap_or_default();

    (impl_generics, ty_generics, wc)
}

/// Strip trait bounds from generic parameters, keeping only the names.
/// `T: Display + Copy` → `T`, `'a: 'b` → `'a`, `const N: usize` → `N`
fn strip_bounds(params: TokenStream2) -> TokenStream2 {
    let mut result = Vec::<TokenTree>::new();
    let mut iter = params.into_iter().peekable();

    while iter.peek().is_some() {
        // Collect one parameter
        let mut param_tokens = Vec::new();

        // Check for lifetime `'a`
        if matches!(iter.peek(), Some(TokenTree::Punct(p)) if p.as_char() == '\'') {
            let tick = iter.next().unwrap();
            param_tokens.push(tick);
            if let Some(name) = iter.next() {
                param_tokens.push(name);
            }
            // Skip `: bound`
            skip_until_comma(&mut iter);
        }
        // Check for `const N: ty`
        else if matches!(iter.peek(), Some(TokenTree::Ident(id)) if *id == "const") {
            iter.next(); // skip `const`
            if let Some(name) = iter.next() {
                param_tokens.push(name);
            }
            // Skip `: type` and any defaults
            skip_until_comma(&mut iter);
        }
        // Regular type parameter `T: Bound`
        else if let Some(TokenTree::Ident(_)) = iter.peek() {
            let name = iter.next().unwrap();
            param_tokens.push(name);
            // Skip `: bounds` and `= Default`
            skip_until_comma(&mut iter);
        } else {
            // Unexpected; just advance
            iter.next();
            continue;
        }

        if !result.is_empty() {
            result.push(TokenTree::Punct(Punct::new(',', Spacing::Alone)));
        }
        result.extend(param_tokens);

        // Consume the comma separator if present
        if matches!(iter.peek(), Some(TokenTree::Punct(p)) if p.as_char() == ',') {
            iter.next();
        }
    }

    result.into_iter().collect()
}

fn skip_until_comma(iter: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>) {
    let mut depth = 0u32;
    while let Some(tok) = iter.peek() {
        match tok {
            TokenTree::Punct(p) if p.as_char() == ',' && depth == 0 => break,
            TokenTree::Punct(p) if p.as_char() == '<' => {
                depth += 1;
                iter.next();
            }
            TokenTree::Punct(p) if p.as_char() == '>' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                iter.next();
            }
            TokenTree::Group(_) => {
                iter.next();
            }
            _ => {
                iter.next();
            }
        }
    }
}

// ============================================================================
// Struct expansion
// ============================================================================

fn expand_struct(
    name: &Ident,
    impl_generics: &TokenStream2,
    ty_generics: &TokenStream2,
    where_clause: &TokenStream2,
    attrs: &[Attr],
    fields: &FieldSet,
) -> Result<TokenStream2, TokenStream2> {
    let transparent = is_transparent(attrs);

    // -- Display --
    let display_impl = if transparent {
        let field = single_field(fields, name.span())?;
        let accessor = field_accessor(&field.0, field.1);
        quote! {
            #[automatically_derived]
            impl #impl_generics ::core::fmt::Display for #name #ty_generics #where_clause {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    ::core::fmt::Display::fmt(&self.#accessor, f)
                }
            }
        }
    } else if let Some(fmt) = find_error_message(attrs)? {
        let body = format_string_to_display_body(&fmt, fields)?;
        quote! {
            #[automatically_derived]
            impl #impl_generics ::core::fmt::Display for #name #ty_generics #where_clause {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    #body
                }
            }
        }
    } else {
        // No #[error] attribute — caller must provide Display manually.
        TokenStream2::new()
    };

    // -- source / from / backtrace analysis --
    let source_field = if transparent {
        Some(single_field(fields, name.span())?)
    } else {
        find_source_field(fields)
    };
    let from_field = find_from_field(fields);
    let backtrace_field = find_backtrace_field(fields);

    // -- Error impl --
    let source_body = if let Some(ref sf) = source_field {
        let accessor = field_accessor(&sf.0, sf.1);
        quote! {
            fn source(&self) -> ::core::option::Option<&(dyn ::core::error::Error + 'static)> {
                ::core::option::Option::Some(&self.#accessor)
            }
        }
    } else {
        TokenStream2::new()
    };

    let provide_body = generate_provide_struct(&source_field, &backtrace_field);

    let error_impl = quote! {
        #[automatically_derived]
        impl #impl_generics ::core::error::Error for #name #ty_generics #where_clause {
            #source_body
            #provide_body
        }
    };

    // -- From impl --
    let from_impl = if let Some(ref ff) = from_field {
        let ty = &ff.2;
        let construct = struct_construct_from(fields, ff.1, &backtrace_field);
        quote! {
            #[automatically_derived]
            impl #impl_generics ::core::convert::From<#ty> for #name #ty_generics #where_clause {
                fn from(source: #ty) -> Self {
                    #construct
                }
            }
        }
    } else {
        TokenStream2::new()
    };

    Ok(quote! {
        #display_impl
        #error_impl
        #from_impl
    })
}

// ============================================================================
// Enum expansion
// ============================================================================

fn expand_enum(
    name: &Ident,
    impl_generics: &TokenStream2,
    ty_generics: &TokenStream2,
    where_clause: &TokenStream2,
    variants: &[Variant],
) -> Result<TokenStream2, TokenStream2> {
    let mut display_arms = Vec::new();
    let mut source_arms = Vec::new();
    let mut from_impls = Vec::new();
    let mut provide_arms = Vec::new();

    for variant in variants {
        let vname = &variant.ident;
        let fields = &variant.fields;
        let transparent = is_transparent(&variant.attrs);

        // -- Display arm --
        if transparent {
            let field = single_field(fields, variant.ident.span())?;
            let (pat, bindings) = variant_pattern(name, vname, fields);
            let binding = &bindings[field.1];
            display_arms.push(quote! {
                #pat => ::core::fmt::Display::fmt(#binding, f),
            });
        } else if let Some(fmt) = find_error_message(&variant.attrs)? {
            let (pat, bindings) = variant_pattern(name, vname, fields);
            let body = format_string_to_write(&fmt, fields, &bindings)?;
            display_arms.push(quote! {
                #pat => { #body }
            });
        } else {
            // No message — use variant name as fallback.
            let (pat, _bindings) = variant_pattern(name, vname, fields);
            let msg = vname.to_string();
            display_arms.push(quote! {
                #pat => f.write_str(#msg),
            });
        }

        // -- Source arm --
        let source_field = if transparent {
            Some(single_field(fields, variant.ident.span())?)
        } else {
            find_source_field(fields)
        };

        if let Some(ref sf) = source_field {
            let (pat, bindings) = variant_pattern(name, vname, fields);
            let binding = &bindings[sf.1];
            source_arms.push(quote! {
                #pat => ::core::option::Option::Some(#binding),
            });
        }

        // -- Provide arm --
        let backtrace_field_v = find_backtrace_field(fields);
        let provide_tokens =
            generate_provide_enum_arm(name, vname, fields, &source_field, &backtrace_field_v);
        if !provide_tokens.is_empty() {
            provide_arms.push(provide_tokens);
        }

        // -- From impl --
        let from_field = find_from_field(fields);
        if let Some(ref ff) = from_field {
            let ty = &ff.2;
            let construct = enum_construct_from(name, vname, fields, ff.1, &backtrace_field_v);
            from_impls.push(quote! {
                #[automatically_derived]
                impl #impl_generics ::core::convert::From<#ty> for #name #ty_generics #where_clause {
                    fn from(source: #ty) -> Self {
                        #construct
                    }
                }
            });
        }
    }

    // -- Display impl --
    let display_impl = if display_arms.is_empty() {
        TokenStream2::new()
    } else {
        quote! {
            #[automatically_derived]
            impl #impl_generics ::core::fmt::Display for #name #ty_generics #where_clause {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    match self {
                        #(#display_arms)*
                    }
                }
            }
        }
    };

    // -- Error impl --
    let source_method = if source_arms.is_empty() {
        TokenStream2::new()
    } else {
        quote! {
            fn source(&self) -> ::core::option::Option<&(dyn ::core::error::Error + 'static)> {
                match self {
                    #(#source_arms)*
                    #[allow(unreachable_patterns)]
                    _ => ::core::option::Option::None,
                }
            }
        }
    };

    let provide_method = if provide_arms.is_empty() {
        TokenStream2::new()
    } else {
        quote! {
            #[cfg(error_generic_member_access)]
            fn provide<'_request>(&'_request self, _request: &mut ::core::error::Request<'_request>) {
                match self {
                    #(#provide_arms)*
                    #[allow(unreachable_patterns)]
                    _ => {}
                }
            }
        }
    };

    let error_impl = quote! {
        #[automatically_derived]
        impl #impl_generics ::core::error::Error for #name #ty_generics #where_clause {
            #source_method
            #provide_method
        }
    };

    Ok(quote! {
        #display_impl
        #error_impl
        #(#from_impls)*
    })
}

// ============================================================================
// Attribute helpers
// ============================================================================

fn is_transparent(attrs: &[Attr]) -> bool {
    attrs.iter().any(|a| {
        a.name == "error"
            && a.tokens
                .as_ref()
                .map(|t| t.to_string().trim() == "transparent")
                .unwrap_or(false)
    })
}

fn find_error_message(attrs: &[Attr]) -> Result<Option<ErrorMessage>, TokenStream2> {
    for attr in attrs {
        if attr.name != "error" {
            continue;
        }
        let tokens = match &attr.tokens {
            Some(t) => t,
            None => continue,
        };
        let s = tokens.to_string();
        if s.trim() == "transparent" {
            return Ok(None);
        }
        return parse_error_message(tokens.clone()).map(Some);
    }
    Ok(None)
}

fn parse_error_message(tokens: TokenStream2) -> Result<ErrorMessage, TokenStream2> {
    let mut iter = tokens.into_iter().peekable();

    // First token should be a string literal.
    let fmt_lit = match iter.next() {
        Some(TokenTree::Literal(lit)) => lit,
        _ => {
            return Err(error(
                Span::call_site(),
                "expected format string in #[error(\"...\")]",
            ));
        }
    };

    // Parse the string value out of the literal repr
    let lit_str = fmt_lit.to_string();
    let fmt_value = if lit_str.starts_with('"') && lit_str.ends_with('"') && lit_str.len() >= 2 {
        unescape_string_literal(&lit_str[1..lit_str.len() - 1])
    } else {
        lit_str.clone()
    };

    // Collect remaining args (comma-separated expressions)
    let mut args = Vec::new();
    while let Some(tok) = iter.peek() {
        if let TokenTree::Punct(p) = tok {
            if p.as_char() == ',' {
                iter.next(); // consume comma
                if iter.peek().is_none() {
                    break; // trailing comma
                }
                // Collect one expression: everything until the next top-level comma or end.
                let mut expr_tokens = Vec::new();
                let mut depth = 0u32;
                while let Some(tok) = iter.peek() {
                    match tok {
                        TokenTree::Punct(p) if p.as_char() == ',' && depth == 0 => break,
                        TokenTree::Group(g)
                            if g.delimiter() == Delimiter::Parenthesis
                                || g.delimiter() == Delimiter::Bracket
                                || g.delimiter() == Delimiter::Brace =>
                        {
                            // Groups are self-contained; no depth tracking needed
                            expr_tokens.push(iter.next().unwrap());
                        }
                        TokenTree::Punct(p) if p.as_char() == '<' => {
                            depth += 1;
                            expr_tokens.push(iter.next().unwrap());
                        }
                        TokenTree::Punct(p) if p.as_char() == '>' => {
                            depth = depth.saturating_sub(1);
                            expr_tokens.push(iter.next().unwrap());
                        }
                        _ => {
                            expr_tokens.push(iter.next().unwrap());
                        }
                    }
                }
                if !expr_tokens.is_empty() {
                    let ts: TokenStream2 = expr_tokens.into_iter().collect();
                    args.push(ts);
                }
            } else {
                break;
            }
        } else {
            break;
        }
    }

    Ok(ErrorMessage {
        fmt_str: fmt_lit,
        fmt_value,
        args,
    })
}

/// Basic unescaping for string literals — handles common escape sequences.
fn unescape_string_literal(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('\\') => result.push('\\'),
                Some('"') => result.push('"'),
                Some('0') => result.push('\0'),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }
    result
}

// ============================================================================
// Field analysis helpers
// ============================================================================

fn fields_iter(fields: &FieldSet) -> &[Field] {
    match fields {
        FieldSet::Named(f) | FieldSet::Unnamed(f) => f,
        FieldSet::Unit => &[],
    }
}

fn has_attr(attrs: &[Attr], name: &str) -> bool {
    attrs.iter().any(|a| a.name == name)
}

fn find_source_field(fields: &FieldSet) -> Option<FieldInfo> {
    for field in fields_iter(fields) {
        if has_attr(&field.attrs, "source") || has_attr(&field.attrs, "from") {
            return Some((field.name.clone(), field.index, field.ty.clone()));
        }
        if let Some(ref ident) = field.name {
            if ident == "source" {
                return Some((Some(ident.clone()), field.index, field.ty.clone()));
            }
        }
    }
    None
}

fn find_from_field(fields: &FieldSet) -> Option<FieldInfo> {
    for field in fields_iter(fields) {
        if has_attr(&field.attrs, "from") {
            return Some((field.name.clone(), field.index, field.ty.clone()));
        }
    }
    None
}

fn find_backtrace_field(fields: &FieldSet) -> Option<FieldInfo> {
    for field in fields_iter(fields) {
        if has_attr(&field.attrs, "backtrace") {
            return Some((field.name.clone(), field.index, field.ty.clone()));
        }
        if type_ends_with_backtrace(&field.ty) {
            return Some((field.name.clone(), field.index, field.ty.clone()));
        }
    }
    None
}

fn type_ends_with_backtrace(ty: &TokenStream2) -> bool {
    let mut last_ident = None;
    for tok in ty.clone().into_iter() {
        if let TokenTree::Ident(id) = tok {
            last_ident = Some(id.to_string());
        }
    }
    last_ident.as_deref() == Some("Backtrace")
}

fn single_field(fields: &FieldSet, span: Span) -> Result<FieldInfo, TokenStream2> {
    let fs = fields_iter(fields);
    if fs.is_empty() {
        return Err(error(span, "transparent requires exactly one field"));
    }

    let first = &fs[0];

    if fs.len() > 1 {
        let second = &fs[1];
        let second_is_backtrace =
            type_ends_with_backtrace(&second.ty) || has_attr(&second.attrs, "backtrace");
        let first_is_backtrace =
            type_ends_with_backtrace(&first.ty) || has_attr(&first.attrs, "backtrace");

        if !second_is_backtrace && !first_is_backtrace {
            return Err(error(
                span,
                "transparent requires exactly one non-backtrace field",
            ));
        }
        if fs.len() > 2 {
            return Err(error(
                span,
                "transparent requires at most two fields (source + backtrace)",
            ));
        }
        if first_is_backtrace {
            return Ok((second.name.clone(), 1, second.ty.clone()));
        }
    }

    Ok((first.name.clone(), 0, first.ty.clone()))
}

fn field_accessor(ident: &Option<Ident>, idx: usize) -> TokenStream2 {
    match ident {
        Some(id) => quote!(#id),
        None => {
            let index = proc_macro2::Literal::usize_unsuffixed(idx);
            quote!(#index)
        }
    }
}

// ============================================================================
// Format string → Display body generation
// ============================================================================

fn format_string_to_display_body(
    msg: &ErrorMessage,
    fields: &FieldSet,
) -> Result<TokenStream2, TokenStream2> {
    let fmt_str = &msg.fmt_str;
    let extra_args = rewrite_dot_args(&msg.args);
    let field_args = extract_field_args(&msg.fmt_value, fields);

    Ok(quote! {
        write!(f, #fmt_str, #(#field_args,)* #(#extra_args,)*)
    })
}

fn format_string_to_write(
    msg: &ErrorMessage,
    fields: &FieldSet,
    bindings: &[Ident],
) -> Result<TokenStream2, TokenStream2> {
    let fmt_str = &msg.fmt_str;
    let extra_args = rewrite_dot_args_enum(&msg.args, fields, bindings);
    let field_args = extract_field_args_enum(&msg.fmt_value, fields, bindings);

    Ok(quote! {
        write!(f, #fmt_str, #(#field_args,)* #(#extra_args,)*)
    })
}

fn extract_field_args(fmt_value: &str, fields: &FieldSet) -> Vec<TokenStream2> {
    let mut args = Vec::new();
    let field_names = collect_field_names(fields);

    for placeholder in parse_placeholders(fmt_value) {
        let base = placeholder.split(':').next().unwrap_or(&placeholder);
        if base.is_empty() {
            continue;
        }
        if base.parse::<usize>().is_ok() {
            continue;
        }
        if field_names.contains(&base.to_string()) {
            let id = format_ident!("{}", base);
            args.push(quote!(#id = &self.#id));
        }
    }

    let max_idx = max_numeric_placeholder(fmt_value, field_count(fields));
    if let Some(max) = max_idx {
        for i in 0..=max {
            let index = proc_macro2::Literal::usize_unsuffixed(i);
            args.insert(i, quote!(&self.#index));
        }
    }

    args
}

fn extract_field_args_enum(
    fmt_value: &str,
    fields: &FieldSet,
    bindings: &[Ident],
) -> Vec<TokenStream2> {
    let mut args = Vec::new();
    let field_names = collect_field_names(fields);

    for placeholder in parse_placeholders(fmt_value) {
        let base = placeholder.split(':').next().unwrap_or(&placeholder);
        if base.is_empty() {
            continue;
        }
        if base.parse::<usize>().is_ok() {
            continue;
        }
        if field_names.contains(&base.to_string()) {
            if let Some(binding) = find_binding_for_name(fields, bindings, base) {
                let id = format_ident!("{}", base);
                args.push(quote!(#id = #binding));
            }
        }
    }

    let max_idx = max_numeric_placeholder(fmt_value, bindings.len());
    if let Some(max) = max_idx {
        for i in 0..=max {
            if i < bindings.len() {
                let binding = &bindings[i];
                args.insert(i, quote!(#binding));
            }
        }
    }

    args
}

/// Rewrite extra args that contain `.field` references for struct context.
fn rewrite_dot_args(args: &[TokenStream2]) -> Vec<TokenStream2> {
    args.iter()
        .map(|expr| {
            let s = expr.to_string();
            // If the expression starts with `.`, it's a field reference.
            if s.starts_with('.') {
                let rewritten = format!("self{}", s);
                if let Ok(ts) = rewritten.parse::<TokenStream2>() {
                    return ts;
                }
            }
            // Check for `name = .field` pattern
            if let Some(eq_pos) = s.find('=') {
                let lhs = s[..eq_pos].trim();
                let rhs = s[eq_pos + 1..].trim();
                if rhs.starts_with('.') {
                    let rewritten_rhs = format!("self{}", rhs);
                    let full = format!("{} = {}", lhs, rewritten_rhs);
                    if let Ok(ts) = full.parse::<TokenStream2>() {
                        return ts;
                    }
                }
            }
            expr.clone()
        })
        .collect()
}

/// Rewrite extra args that contain `.field` references for enum context.
fn rewrite_dot_args_enum(
    args: &[TokenStream2],
    fields: &FieldSet,
    bindings: &[Ident],
) -> Vec<TokenStream2> {
    args.iter()
        .map(|expr| {
            let s = expr.to_string();
            if let Some(without_dot) = s.strip_prefix('.') {
                let root = without_dot.split('.').next().unwrap_or(without_dot);
                if let Some(binding) = find_binding_for_name_or_idx(fields, bindings, root) {
                    let rest = &without_dot[root.len()..];
                    let rewritten = format!("{}{}", binding, rest);
                    if let Ok(ts) = rewritten.parse::<TokenStream2>() {
                        return ts;
                    }
                }
            }
            // Check for `name = .field` pattern
            if let Some(eq_pos) = s.find('=') {
                let lhs = s[..eq_pos].trim();
                let rhs = s[eq_pos + 1..].trim();
                if let Some(without_dot) = rhs.strip_prefix('.') {
                    let root = without_dot.split('.').next().unwrap_or(without_dot);
                    if let Some(binding) = find_binding_for_name_or_idx(fields, bindings, root) {
                        let rest = &without_dot[root.len()..];
                        let rewritten_rhs = format!("{}{}", binding, rest);
                        let full = format!("{} = {}", lhs, rewritten_rhs);
                        if let Ok(ts) = full.parse::<TokenStream2>() {
                            return ts;
                        }
                    }
                }
            }
            expr.clone()
        })
        .collect()
}

// ============================================================================
// Pattern generation for enum variants
// ============================================================================

fn variant_pattern(
    enum_name: &Ident,
    variant_name: &Ident,
    fields: &FieldSet,
) -> (TokenStream2, Vec<Ident>) {
    match fields {
        FieldSet::Named(f) => {
            let mut bindings = Vec::new();
            let mut pats = Vec::new();
            for field in f {
                let name = field.name.as_ref().unwrap();
                let binding = format_ident!("__self_{}", name);
                pats.push(quote!(#name: ref #binding));
                bindings.push(binding);
            }
            (quote!(#enum_name::#variant_name { #(#pats),* }), bindings)
        }
        FieldSet::Unnamed(f) => {
            let mut bindings = Vec::new();
            let mut pats = Vec::new();
            for (i, _field) in f.iter().enumerate() {
                let binding = format_ident!("__self_{}", i);
                pats.push(quote!(ref #binding));
                bindings.push(binding);
            }
            (quote!(#enum_name::#variant_name(#(#pats),*)), bindings)
        }
        FieldSet::Unit => (quote!(#enum_name::#variant_name), Vec::new()),
    }
}

// ============================================================================
// From impl constructors
// ============================================================================

fn struct_construct_from(
    fields: &FieldSet,
    from_idx: usize,
    backtrace_field: &Option<FieldInfo>,
) -> TokenStream2 {
    match fields {
        FieldSet::Named(f) => {
            let inits: Vec<_> = f
                .iter()
                .enumerate()
                .map(|(i, field)| {
                    let name = field.name.as_ref().unwrap();
                    if i == from_idx {
                        quote!(#name: source)
                    } else if backtrace_field.as_ref().map(|(_, bi, _)| *bi) == Some(i) {
                        let bt_ty = &field.ty;
                        quote!(#name: #bt_ty::capture())
                    } else {
                        quote!(#name: ::core::default::Default::default())
                    }
                })
                .collect();
            quote!(Self { #(#inits),* })
        }
        FieldSet::Unnamed(f) => {
            let inits: Vec<_> = f
                .iter()
                .enumerate()
                .map(|(i, field)| {
                    if i == from_idx {
                        quote!(source)
                    } else if backtrace_field.as_ref().map(|(_, bi, _)| *bi) == Some(i) {
                        let bt_ty = &field.ty;
                        quote!(#bt_ty::capture())
                    } else {
                        quote!(::core::default::Default::default())
                    }
                })
                .collect();
            quote!(Self(#(#inits),*))
        }
        FieldSet::Unit => quote!(Self),
    }
}

fn enum_construct_from(
    enum_name: &Ident,
    variant_name: &Ident,
    fields: &FieldSet,
    from_idx: usize,
    backtrace_field: &Option<FieldInfo>,
) -> TokenStream2 {
    match fields {
        FieldSet::Named(f) => {
            let inits: Vec<_> = f
                .iter()
                .enumerate()
                .map(|(i, field)| {
                    let name = field.name.as_ref().unwrap();
                    if i == from_idx {
                        quote!(#name: source)
                    } else if backtrace_field.as_ref().map(|(_, bi, _)| *bi) == Some(i) {
                        let bt_ty = &field.ty;
                        quote!(#name: #bt_ty::capture())
                    } else {
                        quote!(#name: ::core::default::Default::default())
                    }
                })
                .collect();
            quote!(#enum_name::#variant_name { #(#inits),* })
        }
        FieldSet::Unnamed(f) => {
            let inits: Vec<_> = f
                .iter()
                .enumerate()
                .map(|(i, field)| {
                    if i == from_idx {
                        quote!(source)
                    } else if backtrace_field.as_ref().map(|(_, bi, _)| *bi) == Some(i) {
                        let bt_ty = &field.ty;
                        quote!(#bt_ty::capture())
                    } else {
                        quote!(::core::default::Default::default())
                    }
                })
                .collect();
            quote!(#enum_name::#variant_name(#(#inits),*))
        }
        FieldSet::Unit => quote!(#enum_name::#variant_name),
    }
}

// ============================================================================
// Provide (backtrace) generation
// ============================================================================

fn generate_provide_struct(
    source_field: &Option<FieldInfo>,
    backtrace_field: &Option<FieldInfo>,
) -> TokenStream2 {
    if backtrace_field.is_none() {
        return TokenStream2::new();
    }

    let bt = backtrace_field.as_ref().unwrap();
    let bt_accessor = field_accessor(&bt.0, bt.1);

    let is_source_backtrace = match (source_field, backtrace_field) {
        (Some(s), Some(b)) => s.1 == b.1,
        _ => false,
    };

    let body = if is_source_backtrace {
        let src_accessor = field_accessor(
            &source_field.as_ref().unwrap().0,
            source_field.as_ref().unwrap().1,
        );
        quote! {
            ::core::error::Error::provide(&self.#src_accessor, _request);
        }
    } else {
        quote! {
            _request.provide_ref::<::std::backtrace::Backtrace>(&self.#bt_accessor);
        }
    };

    quote! {
        #[cfg(error_generic_member_access)]
        fn provide<'_request>(&'_request self, _request: &mut ::core::error::Request<'_request>) {
            #body
        }
    }
}

fn generate_provide_enum_arm(
    enum_name: &Ident,
    variant_name: &Ident,
    fields: &FieldSet,
    source_field: &Option<FieldInfo>,
    backtrace_field: &Option<FieldInfo>,
) -> TokenStream2 {
    if backtrace_field.is_none() {
        return TokenStream2::new();
    }

    let bt = backtrace_field.as_ref().unwrap();
    let (pat, bindings) = variant_pattern(enum_name, variant_name, fields);
    let bt_binding = &bindings[bt.1];

    let is_source_backtrace = match (source_field, backtrace_field) {
        (Some(s), Some(b)) => s.1 == b.1,
        _ => false,
    };

    let body = if is_source_backtrace {
        let src_binding = &bindings[source_field.as_ref().unwrap().1];
        quote! {
            ::core::error::Error::provide(#src_binding, _request);
        }
    } else {
        quote! {
            _request.provide_ref::<::std::backtrace::Backtrace>(#bt_binding);
        }
    };

    quote! {
        #pat => { #body }
    }
}

// ============================================================================
// Placeholder parsing utilities
// ============================================================================

// FIXME: This function is run twice
fn parse_placeholders(fmt: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            if chars.peek() == Some(&'{') {
                chars.next(); // escaped
                continue;
            }
            let mut name = String::new();
            for c2 in chars.by_ref() {
                if c2 == '}' {
                    break;
                }
                name.push(c2);
            }
            if !name.is_empty() {
                results.push(name.chars().take_while(|c| *c != ':').collect());
            }
        }
    }
    results
}

fn collect_field_names(fields: &FieldSet) -> Vec<String> {
    match fields {
        FieldSet::Named(f) => f
            .iter()
            .filter_map(|f| f.name.as_ref().map(|i| i.to_string()))
            .collect(),
        _ => Vec::new(),
    }
}

fn field_count(fields: &FieldSet) -> usize {
    fields_iter(fields).len()
}

fn max_numeric_placeholder(fmt: &str, field_count: usize) -> Option<usize> {
    let mut max = None;
    for p in parse_placeholders(fmt) {
        if let Ok(idx) = p.parse::<usize>() {
            if idx < field_count {
                max = Some(max.map_or(idx, |m: usize| m.max(idx)));
            }
        }
    }
    max
}

fn find_binding_for_name(fields: &FieldSet, bindings: &[Ident], name: &str) -> Option<Ident> {
    if let FieldSet::Named(f) = fields {
        for (i, field) in f.iter().enumerate() {
            if field.name.as_ref().map(|id| id == name).unwrap_or(false) {
                return Some(bindings[i].clone());
            }
        }
    }
    None
}

fn find_binding_for_name_or_idx(
    fields: &FieldSet,
    bindings: &[Ident],
    name: &str,
) -> Option<Ident> {
    if let Some(b) = find_binding_for_name(fields, bindings, name) {
        return Some(b);
    }
    if let Ok(idx) = name.parse::<usize>() {
        if idx < bindings.len() {
            return Some(bindings[idx].clone());
        }
    }
    None
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn placeholder() {
        let v: Vec<String> = parse_placeholders("{{}}");
        assert_eq!(v, Vec::<String>::new());
        let v: Vec<String> = parse_placeholders("Hello world! ");
        assert_eq!(v, Vec::<String>::new());
        let v: Vec<String> = parse_placeholders("Hello {world}");
        assert_eq!(v, vec!["world"]);
        let v: Vec<String> = parse_placeholders("Hello {world} and {you}");
        assert_eq!(v, vec!["world", "you"]);
        let v: Vec<String> = parse_placeholders("Hello '{world}' and {you}");
        assert_eq!(v, vec!["world", "you"]);
        let v: Vec<String> = parse_placeholders("Hello '{world:?}' and {you}");
        assert_eq!(v, vec!["world", "you"]);
        let v: Vec<String> = parse_placeholders("Hello '{world}': and {you} : and {{I}}, and {I}");
        assert_eq!(v, vec!["world", "you", "I"]);
    }
}
