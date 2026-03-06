use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{ToTokens, format_ident, quote};
use syn::{
    Attribute, Data, DeriveInput, Expr, Fields, Ident, LitStr, Meta, Token, Type,
    parse_macro_input, spanned::Spanned,
};

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

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
    let input = parse_macro_input!(input as DeriveInput);
    match expand(input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand(input: DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    match &input.data {
        Data::Struct(data) => expand_struct(
            name,
            &impl_generics,
            &ty_generics,
            where_clause,
            &input.attrs,
            &data.fields,
        ),
        Data::Enum(data) => expand_enum(
            name,
            &impl_generics,
            &ty_generics,
            where_clause,
            &input.attrs,
            &data.variants,
        ),
        Data::Union(_) => Err(syn::Error::new_spanned(
            &input,
            "derive(Error) does not support unions",
        )),
    }
}

// ---------------------------------------------------------------------------
// Struct expansion
// ---------------------------------------------------------------------------

fn expand_struct(
    name: &Ident,
    impl_generics: &syn::ImplGenerics,
    ty_generics: &syn::TypeGenerics,
    where_clause: Option<&syn::WhereClause>,
    attrs: &[Attribute],
    fields: &Fields,
) -> syn::Result<TokenStream2> {
    let transparent = is_transparent(attrs)?;

    // -- Display --
    let display_impl = if transparent {
        let field = single_field(fields, name.span())?;
        let accessor = field_accessor(&field.0, 0);
        quote! {
            #[automatically_derived]
            impl #impl_generics ::core::fmt::Display for #name #ty_generics #where_clause {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    ::core::fmt::Display::fmt(&self.#accessor, f)
                }
            }
        }
    } else if let Some(fmt) = find_error_message(attrs)? {
        let body = format_string_to_display_body(&fmt, fields, true)?;
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
        let field = single_field(fields, name.span())?;
        Some(field)
    } else {
        find_source_field(fields)?
    };
    let from_field = find_from_field(fields)?;
    let backtrace_field = find_backtrace_field(fields)?;

    // -- Error impl --
    let source_body = if let Some((ref ident_or_idx, idx, ref _ty)) = source_field {
        let accessor = field_accessor(ident_or_idx, idx);
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
    let from_impl = if let Some((ref ident_or_idx, idx, ref ty)) = from_field {
        let construct = struct_construct_from(fields, ident_or_idx, idx, &backtrace_field);
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

// ---------------------------------------------------------------------------
// Enum expansion
// ---------------------------------------------------------------------------

fn expand_enum(
    name: &Ident,
    impl_generics: &syn::ImplGenerics,
    ty_generics: &syn::TypeGenerics,
    where_clause: Option<&syn::WhereClause>,
    _attrs: &[Attribute],
    variants: &syn::punctuated::Punctuated<syn::Variant, Token![,]>,
) -> syn::Result<TokenStream2> {
    let mut display_arms = Vec::new();
    let mut source_arms = Vec::new();
    let mut from_impls = Vec::new();
    let mut provide_arms = Vec::new();

    for variant in variants {
        let vname = &variant.ident;
        let fields = &variant.fields;
        let transparent = is_transparent(&variant.attrs)?;

        // -- Display arm --
        if transparent {
            let field = single_field(fields, variant.span())?;
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
            let field = single_field(fields, variant.span())?;
            Some(field)
        } else {
            find_source_field(fields)?
        };

        if let Some((ref _ident_or_idx, idx, ref _ty)) = source_field {
            let (pat, bindings) = variant_pattern(name, vname, fields);
            let binding = &bindings[idx];
            source_arms.push(quote! {
                #pat => ::core::option::Option::Some(#binding),
            });
        }

        // -- Provide arm --
        let backtrace_field_v = find_backtrace_field(fields)?;
        let provide_tokens =
            generate_provide_enum_arm(name, vname, fields, &source_field, &backtrace_field_v);
        if !provide_tokens.is_empty() {
            provide_arms.push(provide_tokens);
        }

        // -- From impl --
        let from_field = find_from_field(fields)?;
        if let Some((ref ident_or_idx, idx, ref ty)) = from_field {
            let construct =
                enum_construct_from(name, vname, fields, ident_or_idx, idx, &backtrace_field_v);
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

// ---------------------------------------------------------------------------
// Attribute helpers
// ---------------------------------------------------------------------------

/// Check for `#[error(transparent)]`.
fn is_transparent(attrs: &[Attribute]) -> syn::Result<bool> {
    for attr in attrs {
        if !attr.path().is_ident("error") {
            continue;
        }
        if let Meta::List(ref list) = attr.meta {
            let tokens = list.tokens.to_string();
            if tokens.trim() == "transparent" {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Extract the format string from `#[error("...")]` (non-transparent).
fn find_error_message(attrs: &[Attribute]) -> syn::Result<Option<ErrorMessage>> {
    for attr in attrs {
        if !attr.path().is_ident("error") {
            continue;
        }
        match &attr.meta {
            Meta::List(list) => {
                let tokens = list.tokens.to_string();
                if tokens.trim() == "transparent" {
                    return Ok(None);
                }
                let parsed: ErrorMessage = syn::parse2(list.tokens.clone())?;
                return Ok(Some(parsed));
            }
            _ => {}
        }
    }
    Ok(None)
}

/// Parsed contents of `#[error("format string", args...)]`.
struct ErrorMessage {
    fmt: LitStr,
    args: Vec<Expr>,
}

impl syn::parse::Parse for ErrorMessage {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let fmt: LitStr = input.parse()?;
        let mut args = Vec::new();
        while input.peek(Token![,]) {
            let _comma: Token![,] = input.parse()?;
            if input.is_empty() {
                break;
            }
            let arg: Expr = input.parse()?;
            args.push(arg);
        }
        Ok(ErrorMessage { fmt, args })
    }
}

// ---------------------------------------------------------------------------
// Field analysis helpers
// ---------------------------------------------------------------------------

/// (Option<Ident>, positional_index, Type)
type FieldInfo = (Option<Ident>, usize, Type);

fn has_attr(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|a| a.path().is_ident(name))
}

fn find_source_field(fields: &Fields) -> syn::Result<Option<FieldInfo>> {
    let iter = match fields {
        Fields::Named(f) => f.named.iter(),
        Fields::Unnamed(f) => f.unnamed.iter(),
        Fields::Unit => return Ok(None),
    };
    for (i, field) in iter.enumerate() {
        if has_attr(&field.attrs, "source") || has_attr(&field.attrs, "from") {
            return Ok(Some((field.ident.clone(), i, field.ty.clone())));
        }
        if let Some(ref ident) = field.ident {
            if ident == "source" {
                return Ok(Some((Some(ident.clone()), i, field.ty.clone())));
            }
        }
    }
    Ok(None)
}

fn find_from_field(fields: &Fields) -> syn::Result<Option<FieldInfo>> {
    let iter = match fields {
        Fields::Named(f) => f.named.iter(),
        Fields::Unnamed(f) => f.unnamed.iter(),
        Fields::Unit => return Ok(None),
    };
    for (i, field) in iter.enumerate() {
        if has_attr(&field.attrs, "from") {
            return Ok(Some((field.ident.clone(), i, field.ty.clone())));
        }
    }
    Ok(None)
}

fn find_backtrace_field(fields: &Fields) -> syn::Result<Option<FieldInfo>> {
    let iter = match fields {
        Fields::Named(f) => f.named.iter(),
        Fields::Unnamed(f) => f.unnamed.iter(),
        Fields::Unit => return Ok(None),
    };
    for (i, field) in iter.enumerate() {
        if has_attr(&field.attrs, "backtrace") {
            return Ok(Some((field.ident.clone(), i, field.ty.clone())));
        }
        // Auto-detect by type name ending in `Backtrace`.
        if type_ends_with(&field.ty, "Backtrace") {
            return Ok(Some((field.ident.clone(), i, field.ty.clone())));
        }
    }
    Ok(None)
}

fn type_ends_with(ty: &Type, suffix: &str) -> bool {
    if let Type::Path(ref tp) = ty {
        if let Some(seg) = tp.path.segments.last() {
            return seg.ident == suffix;
        }
    }
    false
}

fn single_field(fields: &Fields, span: proc_macro2::Span) -> syn::Result<FieldInfo> {
    let mut iter: Box<dyn Iterator<Item = &syn::Field>> = match fields {
        Fields::Named(f) => Box::new(f.named.iter()),
        Fields::Unnamed(f) => Box::new(f.unnamed.iter()),
        Fields::Unit => {
            return Err(syn::Error::new(
                span,
                "transparent requires exactly one field",
            ));
        }
    };
    let first = iter
        .next()
        .ok_or_else(|| syn::Error::new(span, "transparent requires exactly one field"))?;

    // For transparent, we allow a second field only if it is a Backtrace.
    if let Some(second) = iter.next() {
        let second_is_backtrace =
            type_ends_with(&second.ty, "Backtrace") || has_attr(&second.attrs, "backtrace");
        let first_is_backtrace =
            type_ends_with(&first.ty, "Backtrace") || has_attr(&first.attrs, "backtrace");
        if !second_is_backtrace && !first_is_backtrace {
            return Err(syn::Error::new(
                span,
                "transparent requires exactly one non-backtrace field",
            ));
        }
        if iter.next().is_some() {
            return Err(syn::Error::new(
                span,
                "transparent requires at most two fields (source + backtrace)",
            ));
        }
        // Return the non-backtrace field.
        if first_is_backtrace {
            return Ok((second.ident.clone(), 1, second.ty.clone()));
        }
    }
    Ok((first.ident.clone(), 0, first.ty.clone()))
}

fn field_accessor(ident: &Option<Ident>, idx: usize) -> TokenStream2 {
    match ident {
        Some(id) => quote!(#id),
        None => {
            let index = syn::Index::from(idx);
            quote!(#index)
        }
    }
}

// ---------------------------------------------------------------------------
// Format string → Display body generation
// ---------------------------------------------------------------------------

/// Generate the body of `Display::fmt` for a struct (where fields
/// are accessed via `self.field`).
fn format_string_to_display_body(
    msg: &ErrorMessage,
    fields: &Fields,
    _is_struct: bool,
) -> syn::Result<TokenStream2> {
    let fmt_str = &msg.fmt;
    let extra_args = rewrite_dot_args(&msg.args);

    // Collect named/indexed fields referenced in the shorthand portion of
    // the format string so we can pass them as named arguments to write!.
    let field_args = extract_field_args(&fmt_str.value(), fields);

    Ok(quote! {
        write!(f, #fmt_str, #(#field_args,)* #(#extra_args,)*)
    })
}

/// Generate `write!(f, …)` for an enum arm, where fields are bound
/// to local variable names.
fn format_string_to_write(
    msg: &ErrorMessage,
    fields: &Fields,
    bindings: &[Ident],
) -> syn::Result<TokenStream2> {
    let fmt_str = &msg.fmt;
    let extra_args = rewrite_dot_args_enum(&msg.args, fields, bindings);

    let field_args = extract_field_args_enum(&fmt_str.value(), fields, bindings);

    Ok(quote! {
        write!(f, #fmt_str, #(#field_args,)* #(#extra_args,)*)
    })
}

/// For each `{name}` or `{0}` placeholder that refers to a field,
/// produce `name = self.name` (struct context).
fn extract_field_args(fmt_value: &str, fields: &Fields) -> Vec<TokenStream2> {
    let mut args = Vec::new();
    let field_names = collect_field_names(fields);

    for placeholder in parse_placeholders(fmt_value) {
        let base = placeholder.split(':').next().unwrap_or(&placeholder);
        if base.is_empty() {
            continue;
        }
        // Numeric index?
        if base.parse::<usize>().is_ok() {
            // Numeric placeholders are handled separately below.
            continue;
        }
        // Named field?
        if field_names.contains(&base.to_string()) {
            let id = format_ident!("{}", base);
            args.push(quote!(#id = &self.#id));
        }
    }

    // For numeric placeholders, supply positional args in order.
    let max_idx = max_numeric_placeholder(fmt_value, field_count(fields));
    if let Some(max) = max_idx {
        // We need to provide at least max+1 positional args.
        // Clear named args that might conflict and rebuild.
        // Actually write! supports mixing positional and named fine.
        for i in 0..=max {
            let index = syn::Index::from(i);
            args.insert(i, quote!(&self.#index));
        }
    }

    args
}

/// Same as above but for enum variant arms where fields are bound.
fn extract_field_args_enum(
    fmt_value: &str,
    fields: &Fields,
    bindings: &[Ident],
) -> Vec<TokenStream2> {
    let mut args = Vec::new();
    let field_names = collect_field_names(fields);

    for placeholder in parse_placeholders(fmt_value) {
        let base = placeholder.split(':').next().unwrap_or(&placeholder);
        if base.is_empty() {
            continue;
        }
        if let Ok(_idx) = base.parse::<usize>() {
            continue; // handled below
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
fn rewrite_dot_args(args: &[Expr]) -> Vec<TokenStream2> {
    args.iter()
        .map(|expr| {
            let s = expr.to_token_stream().to_string();
            // If the expression starts with `.`, it's a field reference.
            // e.g. `.limits.lo` → `self.limits.lo`
            if s.starts_with('.') {
                let rewritten = format!("self{}", s);
                // Parse as expression.
                if let Ok(e) = syn::parse_str::<Expr>(&rewritten) {
                    return e.to_token_stream();
                }
            }
            // Check for `name = .field` pattern (named arg).
            if let Expr::Assign(ref assign) = expr {
                let rhs = assign.right.to_token_stream().to_string();
                if rhs.starts_with('.') {
                    let rewritten = format!("self{}", rhs);
                    if let Ok(e) = syn::parse_str::<Expr>(&rewritten) {
                        let lhs = &assign.left;
                        return quote!(#lhs = #e);
                    }
                }
            }
            expr.to_token_stream()
        })
        .collect()
}

/// Rewrite extra args that contain `.field` references for enum context.
fn rewrite_dot_args_enum(args: &[Expr], fields: &Fields, bindings: &[Ident]) -> Vec<TokenStream2> {
    args.iter()
        .map(|expr| {
            let s = expr.to_token_stream().to_string();
            if s.starts_with('.') {
                // `.limits.lo` → rewrite leading field to binding
                let without_dot = &s[1..];
                let root = without_dot.split('.').next().unwrap_or(without_dot);
                if let Some(binding) = find_binding_for_name_or_idx(fields, bindings, root) {
                    // Replace the root with the binding, keep rest.
                    let rest: &str = &without_dot[root.len()..];
                    let rewritten = format!("{}{}", binding, rest);
                    if let Ok(e) = syn::parse_str::<Expr>(&rewritten) {
                        return e.to_token_stream();
                    }
                }
            }
            if let Expr::Assign(ref assign) = expr {
                let rhs = assign.right.to_token_stream().to_string();
                if rhs.starts_with('.') {
                    let without_dot = &rhs[1..];
                    let root = without_dot.split('.').next().unwrap_or(without_dot);
                    if let Some(binding) = find_binding_for_name_or_idx(fields, bindings, root) {
                        let rest: &str = &without_dot[root.len()..];
                        let rewritten = format!("{}{}", binding, rest);
                        if let Ok(e) = syn::parse_str::<Expr>(&rewritten) {
                            let lhs = &assign.left;
                            return quote!(#lhs = #e);
                        }
                    }
                }
            }
            expr.to_token_stream()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Pattern generation for enum variants
// ---------------------------------------------------------------------------

/// Generate a match pattern and a list of binding identifiers for a variant.
fn variant_pattern(
    enum_name: &Ident,
    variant_name: &Ident,
    fields: &Fields,
) -> (TokenStream2, Vec<Ident>) {
    match fields {
        Fields::Named(f) => {
            let mut bindings = Vec::new();
            let mut pats = Vec::new();
            for field in &f.named {
                let name = field.ident.as_ref().unwrap();
                let binding = format_ident!("__self_{}", name);
                pats.push(quote!(#name: ref #binding));
                bindings.push(binding);
            }
            (quote!(#enum_name::#variant_name { #(#pats),* }), bindings)
        }
        Fields::Unnamed(f) => {
            let mut bindings = Vec::new();
            let mut pats = Vec::new();
            for (i, _field) in f.unnamed.iter().enumerate() {
                let binding = format_ident!("__self_{}", i);
                pats.push(quote!(ref #binding));
                bindings.push(binding);
            }
            (quote!(#enum_name::#variant_name(#(#pats),*)), bindings)
        }
        Fields::Unit => (quote!(#enum_name::#variant_name), Vec::new()),
    }
}

// ---------------------------------------------------------------------------
// From impl constructors
// ---------------------------------------------------------------------------

fn struct_construct_from(
    fields: &Fields,
    _from_ident: &Option<Ident>,
    from_idx: usize,
    backtrace_field: &Option<FieldInfo>,
) -> TokenStream2 {
    match fields {
        Fields::Named(f) => {
            let inits: Vec<_> = f
                .named
                .iter()
                .enumerate()
                .map(|(i, field)| {
                    let name = field.ident.as_ref().unwrap();
                    if i == from_idx {
                        quote!(#name: source)
                    } else if backtrace_field.as_ref().map(|(_, bi, _)| *bi) == Some(i) {
                        // If there's a Backtrace field alongside #[from],
                        // capture a backtrace.
                        let bt_ty = &field.ty;
                        quote!(#name: #bt_ty::capture())
                    } else {
                        quote!(#name: ::core::default::Default::default())
                    }
                })
                .collect();
            quote!(Self { #(#inits),* })
        }
        Fields::Unnamed(f) => {
            let inits: Vec<_> = f
                .unnamed
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
        Fields::Unit => quote!(Self),
    }
}

fn enum_construct_from(
    enum_name: &Ident,
    variant_name: &Ident,
    fields: &Fields,
    _from_ident: &Option<Ident>,
    from_idx: usize,
    backtrace_field: &Option<FieldInfo>,
) -> TokenStream2 {
    match fields {
        Fields::Named(f) => {
            let inits: Vec<_> = f
                .named
                .iter()
                .enumerate()
                .map(|(i, field)| {
                    let name = field.ident.as_ref().unwrap();
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
        Fields::Unnamed(f) => {
            let inits: Vec<_> = f
                .unnamed
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
        Fields::Unit => quote!(#enum_name::#variant_name),
    }
}

// ---------------------------------------------------------------------------
// Provide (backtrace) generation
// ---------------------------------------------------------------------------

fn generate_provide_struct(
    source_field: &Option<FieldInfo>,
    backtrace_field: &Option<FieldInfo>,
) -> TokenStream2 {
    if backtrace_field.is_none() {
        return TokenStream2::new();
    }

    let bt = backtrace_field.as_ref().unwrap();
    let bt_accessor = field_accessor(&bt.0, bt.1);

    // If the source field is also the backtrace field (i.e. #[backtrace] on source),
    // forward to source's provide.
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
    fields: &Fields,
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

// ---------------------------------------------------------------------------
// Placeholder parsing utilities
// ---------------------------------------------------------------------------

/// Extract placeholder names from a format string.
/// e.g. "hello {name} and {0:?}" → ["name", "0"]
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
                if c2 == '}' || c2 == ':' {
                    break;
                }
                name.push(c2);
            }
            if !name.is_empty() {
                results.push(name);
            }
            // Consume until '}'
            if fmt.contains(':') {
                for c2 in chars.by_ref() {
                    if c2 == '}' {
                        break;
                    }
                }
            }
        }
    }
    results
}

fn collect_field_names(fields: &Fields) -> Vec<String> {
    match fields {
        Fields::Named(f) => f
            .named
            .iter()
            .filter_map(|f| f.ident.as_ref().map(|i| i.to_string()))
            .collect(),
        _ => Vec::new(),
    }
}

fn field_count(fields: &Fields) -> usize {
    match fields {
        Fields::Named(f) => f.named.len(),
        Fields::Unnamed(f) => f.unnamed.len(),
        Fields::Unit => 0,
    }
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

fn find_binding_for_name(fields: &Fields, bindings: &[Ident], name: &str) -> Option<Ident> {
    if let Fields::Named(f) = fields {
        for (i, field) in f.named.iter().enumerate() {
            if field.ident.as_ref().map(|id| id == name).unwrap_or(false) {
                return Some(bindings[i].clone());
            }
        }
    }
    None
}

fn find_binding_for_name_or_idx(fields: &Fields, bindings: &[Ident], name: &str) -> Option<Ident> {
    // Try named first.
    if let Some(b) = find_binding_for_name(fields, bindings, name) {
        return Some(b);
    }
    // Try numeric index.
    if let Ok(idx) = name.parse::<usize>() {
        if idx < bindings.len() {
            return Some(bindings[idx].clone());
        }
    }
    None
}
