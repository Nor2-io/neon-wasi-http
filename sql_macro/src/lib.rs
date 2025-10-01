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
            let mut child_type = None;
            if let Type::Path(type_path) = &field.ty {
                if let Some(segment) = type_path.path.segments.last() {
                    if segment.ident == "Vec" {
                        if let PathArguments::AngleBracketed(args) = &segment.arguments {
                            if let Some(GenericArgument::Type(ty)) = args.args.first() {
                                child_type = Some(ty.clone());
                            }
                        }
                    }
                }
            }
            if let Some(ty) = child_type {
                related_fields.push((field_ident.clone(), ty));
            } else {
                panic!("`is_related` attribute can only be used on a Vec<T> field");
            }
        } else {
            simple_field_idents.push(field_ident.clone());
        }
    }

    // --- Insert Logic ---
    let insert_impl = if !related_fields.is_empty() {
        let child_inserts_gen = related_fields.iter().map(|(rel_ident, child_type)| {
            quote! {
                if !self.#rel_ident.is_empty() {
                    let child_table_name = #child_type::table_name();
                    let child_pk_name = #child_type::pk_column_name();
                    let foreign_key_col = format!("{}_id", #table_name.trim_end_matches('s'));
                    let mut child_cols = vec![foreign_key_col.clone()];
                    let mut child_field_names: Vec<String> = Vec::new();
                    if let Some(first_child) = self.#rel_ident.get(0) {
                        let val = serde_json::to_value(first_child).unwrap();
                        if let Some(obj) = val.as_object() {
                            for key in obj.keys() {
                                if key != child_pk_name && key != &foreign_key_col {
                                    child_field_names.push(key.clone());
                                }
                            }
                            child_field_names.sort();
                            child_cols.extend(child_field_names.iter().cloned());
                        }
                    }
                    let mut child_value_clauses = vec![];
                    for item in &self.#rel_ident {
                        let mut placeholders = vec![];
                        let item_val = serde_json::to_value(item).unwrap();
                        let item_obj = item_val.as_object().unwrap();
                        for field_name in &child_field_names {
                            placeholders.push(format!("${}", combined_params.len() + 1));
                            combined_params.push(item_obj.get(field_name).unwrap_or(&serde_json::Value::Null).clone());
                        }
                        child_value_clauses.push(format!("((SELECT id FROM inserted_parent), {})", placeholders.join(", ")));
                    }
                    let child_sql = format!(
                        "INSERT INTO {} ({}) VALUES {}",
                        child_table_name, child_cols.join(", "), child_value_clauses.join(", ")
                    );
                    all_sql.push(child_sql);
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
                        parent_params.push(val);
                    }
                )*;
                let parent_insert_sql = format!(
                    "WITH inserted_parent AS (INSERT INTO {} ({}) VALUES ({}) RETURNING {})",
                    Self::table_name(), parent_cols.join(", "), parent_placeholders.join(", "), Self::pk_column_name()
                );
                let mut combined_params = parent_params;
                let mut all_sql = vec![parent_insert_sql];

                #(#child_inserts_gen)*;

                (all_sql.join(" "), combined_params)
            }
        }
    } else {
        quote! {
            let mut columns = vec![];
            let mut params = vec![];
            let mut placeholders = vec![];
            #(
                let val = serde_json::to_value(&self.#simple_field_idents).unwrap();
                if !val.is_null() {
                    columns.push(stringify!(#simple_field_idents).to_string());
                    placeholders.push(format!("${}", params.len() + 1));
                    params.push(val);
                }
            )*
            let query = format!(
                "INSERT INTO {} ({}) VALUES ({})",
                Self::table_name(), columns.join(", "), placeholders.join(", ")
            );
            (query, params)
        }
    };

    // --- Update Logic ---
    let update_impl = quote! {
        let mut set_clauses = vec![];
        let mut params = vec![];
        #(
            let val = serde_json::to_value(&self.#simple_field_idents).unwrap();
            if !val.is_null() {
                set_clauses.push(format!("{} = ${}", stringify!(#simple_field_idents), params.len() + 1));
                params.push(val);
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
            Self::pk_column_name(),
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
            Self::pk_column_name()
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

        let related_field_fragments = related_fields.iter().map(|(ident, child_type)| {
            let ident_str = ident.to_string();
            let foreign_key_col = format!("{}_id", table_name.trim_end_matches('s'));
            quote! {
                {
                    let child_table = #child_type::table_name();
                    joins.push(format!(
                        "LEFT JOIN (SELECT {} AS fk, json_agg(to_jsonb(child)) AS items FROM {} AS child GROUP BY fk) AS {} ON a.{} = {}.fk",
                        #foreign_key_col,
                        child_table,
                        #ident_str,
                        Self::pk_column_name(),
                        #ident_str
                    ));
                    selectors.push(format!("'{}', COALESCE({}, '[]'::jsonb)", #ident_str, format!("{}.items", #ident_str)));
                }
            }
        });

        quote! {
            {
                use #trait_path;

                let mut selectors: Vec<String> = Vec::new();
                let mut joins: Vec<String> = Vec::new();

                selectors.push(format!("'{}', a.{}", Self::pk_column_name(), Self::pk_column_name()));

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
