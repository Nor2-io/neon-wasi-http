extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, Fields, GenericArgument, PathArguments, Type};

#[proc_macro_derive(NeonTable, attributes(neon_table))]
pub fn neon_table_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;

    // --- Struct-level attribute parsing ---
    let mut table_name = None;
    let mut pk_name = None;
    let mut crate_path_override = None;
    let mut on_conflict_clause = String::new();

    for attr in &input.attrs {
        if attr.path.is_ident("neon_table") {
            if let Ok(syn::Meta::List(meta_list)) = attr.parse_meta() {
                for nested in meta_list.nested {
                    if let syn::NestedMeta::Meta(syn::Meta::NameValue(nv)) = nested {
                        if nv.path.is_ident("table_name") {
                            if let syn::Lit::Str(lit_str) = nv.lit {
                                table_name = Some(lit_str.value());
                            }
                        } else if nv.path.is_ident("pk") {
                            if let syn::Lit::Str(lit_str) = nv.lit {
                                pk_name = Some(lit_str.value());
                            }
                        } else if nv.path.is_ident("crate_path") {
                            if let syn::Lit::Str(lit_str) = nv.lit {
                                // Store the override path, e.g., "crate"
                                crate_path_override = Some(lit_str.value());
                            }
                        } else if nv.path.is_ident("on_conflict") {
                            if let syn::Lit::Str(lit_str) = nv.lit {
                                let val = lit_str.value();
                                if val.to_lowercase().contains("do nothing") {
                                    on_conflict_clause = " ON CONFLICT DO NOTHING".to_string();
                                } else {
                                    // Assume the user provided just the column name
                                    on_conflict_clause =
                                        format!(" ON CONFLICT ({}) DO NOTHING", val);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let table_name = table_name.expect("`table_name` attribute is required");
    let pk_name_str = pk_name.expect("`pk` attribute is required");
    let pk_ident = format_ident!("{}", pk_name_str);

    let trait_path = if let Some(path_str) = crate_path_override {
        let path: syn::Path = syn::parse_str(&path_str).expect("Failed to parse crate_path");
        quote! { #path::orm::NeonTable }
    } else {
        quote! { ::neon_wasi_http::orm::NeonTable }
    };

    let fields = if let Data::Struct(data) = &input.data {
        &data.fields
    } else {
        panic!("Only structs supported")
    };
    let named_fields = if let Fields::Named(fields) = fields {
        &fields.named
    } else {
        panic!("Only named fields supported")
    };

    let mut simple_field_idents = Vec::new();
    let mut related_fields = Vec::new();
    for field in named_fields {
        let field_ident = field.ident.as_ref().unwrap();
        if field_ident == &pk_ident {
            continue;
        }
        let mut is_related = false;
        for attr in &field.attrs {
            if attr.path.is_ident("neon_table") {
                if let Ok(syn::Meta::List(meta_list)) = attr.parse_meta() {
                    for nested in meta_list.nested {
                        if let syn::NestedMeta::Meta(syn::Meta::Path(path)) = nested {
                            if path.is_ident("is_related") {
                                is_related = true;
                            }
                        }
                    }
                }
            }
        }
        if is_related {
            let mut is_vec = false;
            let mut child_type = None;
            if let Type::Path(type_path) = &field.ty {
                if let Some(segment) = type_path.path.segments.last() {
                    if segment.ident == "Vec" {
                        is_vec = true;
                        if let PathArguments::AngleBracketed(args) = &segment.arguments {
                            if let Some(GenericArgument::Type(ty)) = args.args.first() {
                                child_type = Some(ty.clone());
                            }
                        }
                    } else if segment.ident == "Option" {
                        if let PathArguments::AngleBracketed(args) = &segment.arguments {
                            if let Some(GenericArgument::Type(ty)) = args.args.first() {
                                child_type = Some(ty.clone());
                            }
                        }
                    }
                }
            }
            if let Some(ty) = child_type {
                related_fields.push((field_ident.clone(), ty, is_vec));
            } else {
                panic!("`is_related` attribute can only be used on a Vec<T> or Option<T> fields");
            }
        } else {
            simple_field_idents.push(field_ident.clone());
        }
    }

    // --- Insert Logic ---
    let simple_insert_logic = quote! {
        let mut columns = vec![];
        let mut params = vec![];
        let mut placeholders = vec![];
        #(
            let val = serde_json::to_value(&self.#simple_field_idents).unwrap();
            if !val.is_null() {
                columns.push(stringify!(#simple_field_idents).to_string());
                placeholders.push(format!("${}", params.len() + 1));
                if let serde_json::Value::Object(_) | serde_json::Value::Array(_) = &val {
                    let json_string = serde_json::to_string(&val).unwrap();
                    params.push(serde_json::Value::String(json_string));
                } else {
                    params.push(val);
                }
            }
        )*
        let query = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            Self::table_name(), columns.join(", "), placeholders.join(", ")
        );
        (query, params)
    };

    let insert_impl = if !related_fields.is_empty() {
        // This is the logic for the complex relational insert.
        let relational_insert_logic = {
            let child_inserts_gen = related_fields.iter().map(|(rel_ident, child_type, is_vec)| {
                let rel_ident_str = rel_ident.to_string();

                let (check_has_data, get_sample, get_iter) = if *is_vec {
                    (
                        quote! { !self.#rel_ident.is_empty() },
                        quote! { self.#rel_ident.get(0).unwrap() },
                        quote! { self.#rel_ident.iter() }
                    )
                } else {
                    (
                        quote! { self.#rel_ident.is_some() },
                        quote! { self.#rel_ident.as_ref().unwrap() },
                        quote! { self.#rel_ident.as_ref().into_iter() }
                    )
                };

                quote! {

                    if #check_has_data {
                        let child_table_name = #child_type::table_name();
                        let child_pk_name = #child_type::pk_column_name();
                        let foreign_key_col = format!("{}_id", Self::table_name().trim_end_matches('s'));

                        let mut child_cols = vec![foreign_key_col.clone()];
                        let mut child_field_names: Vec<String> = Vec::new();

                        let sample_item = #get_sample;
                        let item_val = serde_json::to_value(sample_item).unwrap();
                        if let Some(obj) = item_val.as_object() {
                            for key in obj.keys() {
                                if key != child_pk_name && key != &foreign_key_col {
                                    child_field_names.push(key.clone());
                                }
                            }
                            child_field_names.sort();
                            child_cols.extend(child_field_names.iter().cloned());
                        }

                        let mut child_value_clauses = vec![];

                        for item in #get_iter {
                            let item_obj_val = serde_json::to_value(item).unwrap();
                            let item_obj = item_obj_val.as_object().unwrap();
                            let mut placeholders = vec![];

                            for field_name in &child_field_names {
                                placeholders.push(format!("${}", combined_params.len() + 1));
                                let val = item_obj.get(field_name).unwrap_or(&serde_json::Value::Null).clone();

                                if let serde_json::Value::Object(_) | serde_json::Value::Array(_) = &val {
                                    combined_params.push(serde_json::Value::String(serde_json::to_string(&val).unwrap()));
                                } else {
                                    combined_params.push(val);
                                }
                            }
                            child_value_clauses.push(format!("((SELECT id FROM inserted_parent), {})", placeholders.join(", ")));
                        }

                        // Check if the child type has an ON CONFLICT logic (e.g., for media_item)
                        // We assume child_type implements a method `on_conflict_sql()` via the trait
                        let conflict_clause = #child_type::on_conflict_sql();

                        let child_insert = format!(
                            "INSERT INTO {} ({}) VALUES {} {} ",
                            child_table_name,
                            child_cols.join(", "),
                            child_value_clauses.join(", "),
                            conflict_clause
                        );

                        with_clauses.push(format!(", inserted_{} AS ({} RETURNING 1)", #rel_ident_str, child_insert));
                    }
                }
            });

            quote! {
                {
                    use #trait_path;
                    let mut parent_cols = vec![];
                    let mut parent_params = vec![];
                    let mut parent_placeholders = vec![];
                    #(
                        let val = serde_json::to_value(&self.#simple_field_idents).unwrap();
                        if !val.is_null() {
                            parent_cols.push(stringify!(#simple_field_idents).to_string());
                            parent_placeholders.push(format!("${}", parent_params.len() + 1));
                            if let serde_json::Value::Object(_) | serde_json::Value::Array(_) = &val {
                                parent_params.push(serde_json::Value::String(serde_json::to_string(&val).unwrap()));
                            } else {
                                parent_params.push(val);
                            }
                        }
                    )*;

                    let mut combined_params = parent_params;
                    let mut with_clauses = vec![
                        format!(
                            "WITH inserted_parent AS (INSERT INTO {} ({}) VALUES ({}) RETURNING id)",
                            Self::table_name(), parent_cols.join(", "), parent_placeholders.join(", ")
                        )
                    ];

                    #(#child_inserts_gen)*

                    let final_query = format!("{} SELECT id FROM inserted_parent", with_clauses.join(" "));
                    (final_query, combined_params)
                }
            }
        };

        let empty_check_fragments = related_fields.iter().map(|(ident, _, is_vec)| {
            if *is_vec {
                quote! { self.#ident.is_empty() }
            } else {
                quote! { self.#ident.is_none() }
            }
        });

        quote! {
            if #(#empty_check_fragments)&&* {
                #simple_insert_logic
            } else {
                #relational_insert_logic
            }
        }
    } else {
        simple_insert_logic
    };

    // --- Update Logic ---
    let update_impl = quote! {
        let mut set_clauses = vec![];
        let mut params = vec![];
        #(
            let val = serde_json::to_value(&self.#simple_field_idents).unwrap();
            if !val.is_null() {
                set_clauses.push(format!("{} = ${}", stringify!(#simple_field_idents), params.len() + 1));
                if let serde_json::Value::Object(_) | serde_json::Value::Array(_) = &val {
                    let json_string = serde_json::to_string(&val).unwrap();
                    params.push(serde_json::Value::String(json_string));
                } else {
                    params.push(val);
                }
            }
        )*
        let pk_val = self.pk_value();
        if pk_val.is_null() {
            panic!("Cannot update a record without a primary key value.");
        }
        let query = format!(
            "UPDATE {} SET {} WHERE {} = ${}",
            Self::table_name(),
            set_clauses.join(", "),
            #pk_name_str,
            params.len() + 1
        );
        params.push(pk_val);
        (query, params)
    };

    // --- Delete Logic ---
    let delete_impl = quote! {
        let pk_val = self.pk_value();
        if pk_val.is_null() {
            panic!("Cannot delete a record without a primary key value.");
        }
        let query = format!(
            "DELETE FROM {} WHERE {} = $1",
            Self::table_name(),
            #pk_name_str
        );
        (query, vec![pk_val])
    };

    // -- select as json logic --
    let select_as_json_impl = {
        let simple_field_fragments = simple_field_idents.iter().map(|ident| {
            let ident_str = ident.to_string();
            quote! {
                selectors.push(format!("'{}', a.{}", #ident_str, #ident_str));
            }
        });

        let related_field_fragments = related_fields.iter().map(|(ident, child_type, is_vec)| {
            let ident_str = ident.to_string();
            let foreign_key_col = format!("{}_id", table_name.trim_end_matches('s'));

            if *is_vec {
                    quote! {
                        let child_table = #child_type::table_name();
                        joins.push(format!(
                            "LEFT JOIN (SELECT {} AS fk, jsonb_agg(to_jsonb(c)) AS items FROM {} c GROUP BY fk) AS {} ON a.{} = {}.fk",
                            #foreign_key_col, child_table, #ident_str, #pk_name_str, #ident_str
                        ));
                        selectors.push(format!("'{}', COALESCE({}.items, '[]'::jsonb)", #ident_str, #ident_str));
                    }
                } else {
                    // One-to-One Selection
                    quote! {
                        let child_table = #child_type::table_name();
                        joins.push(format!(
                            "LEFT JOIN (SELECT {} AS fk, to_jsonb(c) AS item FROM {} c) AS {} ON a.{} = {}.fk",
                            #foreign_key_col, child_table, #ident_str, #pk_name_str, #ident_str
                        ));
                        selectors.push(format!("'{}', {}.item", #ident_str, #ident_str));
                    }
                }
        });

        quote! {
            {
                use #trait_path;

                let mut selectors: Vec<String> = Vec::new();
                let mut joins: Vec<String> = Vec::new();

                selectors.push(format!("'{}', a.{}", #pk_name_str, #pk_name_str));

                #(#simple_field_fragments)*
                #(#related_field_fragments)*

                format!(
                    "SELECT jsonb_build_object({}) FROM {} a {}",
                    selectors.join(", "),
                    Self::table_name(),
                    joins.join(" ")
                )
            }
        }
    };

    // --- Final `impl` block ---
    let expanded = quote! {
        impl #trait_path for #struct_name {
            fn table_name() -> &'static str { #table_name }
            fn pk_column_name() -> &'static str { #pk_name_str }
            fn on_conflict_sql() -> &'static str { #on_conflict_clause }

            fn pk_value(&self) -> serde_json::Value {
                serde_json::to_value(&self.#pk_ident).expect("PK field must be serializable")
            }

            fn to_sql_insert_transaction(&self) -> (String, Vec<serde_json::Value>) {
                #insert_impl
            }

            fn to_sql_update(&self) -> (String, Vec<serde_json::Value>) {
                #update_impl
            }

            fn to_sql_delete(&self) -> (String, Vec<serde_json::Value>) {
                #delete_impl
            }

            fn select_as_json_sql() -> String {
                #select_as_json_impl
            }


        }
    };

    TokenStream::from(expanded)
}
