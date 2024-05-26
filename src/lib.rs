extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{quote, format_ident};
use syn::{parse_macro_input, ItemStruct, ItemImpl, Lit, ImplItem, ItemFn, Token, Fields, visit_mut, LitInt};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::visit_mut::VisitMut;
use syn::visit_mut::visit_item_impl_mut;

#[proc_macro_attribute]
pub fn generate_extern_fn_simple(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);

    let fn_name = &input_fn.sig.ident;
    let fn_block = &input_fn.block;

    let extern_fn_name = syn::Ident::new(
        &format!("extern_{}", fn_name),
        fn_name.span(),
    );

    let result = quote! {
        #[no_mangle]
        pub extern "C" fn #extern_fn_name(param: u32) {
            #fn_block
        }

        #input_fn
    };

    result.into()
}

// Struct to parse comma-separated strings
struct Args(Vec<String>);

impl Parse for Args {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let args = Punctuated::<Lit, Token![,]>::parse_terminated(input)?;
        let strings = args.into_iter().filter_map(|lit| {
            if let Lit::Str(lit_str) = lit {
                Some(lit_str.value())
            } else {
                None
            }
        }).collect();
        Ok(Args(strings))
    }
}

struct StructNameRewriter {
    old_name: String,
    new_name: String,
}

impl VisitMut for StructNameRewriter {
    fn visit_ident_mut(&mut self, i: &mut syn::Ident) {
        if i == self.old_name.as_str() {
            *i = syn::Ident::new(&self.new_name, i.span());
        }
    }
}

#[proc_macro_attribute]
pub fn generate_struct(args: TokenStream, input: TokenStream) -> TokenStream {
    let input_struct = parse_macro_input!(input as ItemStruct);
    let Args(args) = parse_macro_input!(args as Args);

    let struct_name = &input_struct.ident;
    let vis = &input_struct.vis;
    let fields = &input_struct.fields;

    let mut generated_structs = quote! {};

    for arg in args {
        let ident = format_ident!("{}_{}", struct_name, arg);
        let instance_fields = match fields {
            Fields::Named(ref fields_named) => {
                let field_names = fields_named.named.iter().map(|f| &f.ident);
                quote! {
                    #(#field_names: Default::default(),)*
                }
            },
            Fields::Unnamed(ref fields_unnamed) => {
                let field_defaults = fields_unnamed.unnamed.iter().map(|_| quote! { Default::default() });
                quote! {
                    #(#field_defaults,)*
                }
            },
            Fields::Unit => quote! {},
        };

        generated_structs.extend(match fields {
            Fields::Named(_) => quote! {
                #vis struct #ident #fields

                impl #ident {
                    pub fn instance() -> &'static Self {
                        use std::sync::Once;
                        static mut INSTANCE: *const #ident = std::ptr::null();
                        static INIT: Once = Once::new();

                        INIT.call_once(|| {
                            let instance = #ident {
                                #instance_fields
                            };
                            unsafe {
                                INSTANCE = Box::into_raw(Box::new(instance));
                            }
                        });

                        unsafe {
                            &*INSTANCE
                        }
                    }
                }
            },
            Fields::Unnamed(_) => quote! {
                #vis struct #ident #fields

                impl #ident {
                    pub fn instance() -> &'static Self {
                        use std::sync::Once;
                        static mut INSTANCE: *const #ident = std::ptr::null();
                        static INIT: Once = Once::new();

                        INIT.call_once(|| {
                            let instance = #ident(
                                #instance_fields
                            );
                            unsafe {
                                INSTANCE = Box::into_raw(Box::new(instance));
                            }
                        });

                        unsafe {
                            &*INSTANCE
                        }
                    }
                }
            },
            Fields::Unit => quote! {
                #vis struct #ident;

                impl #ident {
                    pub fn instance() -> &'static Self {
                        use std::sync::Once;
                        static mut INSTANCE: *const #ident = std::ptr::null();
                        static INIT: Once = Once::new();

                        INIT.call_once(|| {
                            let instance = #ident;
                            unsafe {
                                INSTANCE = Box::into_raw(Box::new(instance));
                            }
                        });

                        unsafe {
                            &*INSTANCE
                        }
                    }
                }
            },
        });
    }

    let expanded = quote! {
        #input_struct
        #generated_structs
    };

    TokenStream::from(expanded)
}

#[proc_macro_attribute]
pub fn generate_struct_impl(args: TokenStream, input: TokenStream) -> TokenStream {
    let input_impl = parse_macro_input!(input as ItemImpl);
    let Args(args) = parse_macro_input!(args as Args);

    let self_ty = &input_impl.self_ty;

    let mut generated_impls = quote! {};

    for arg in args {
        let ident_str = format!("{}_{}", quote!(#self_ty).to_string().replace(" ", ""), arg);
        let ident = format_ident!("{}", ident_str);

        let mut new_impl = input_impl.clone();
        let mut rewriter = StructNameRewriter {
            old_name: quote!(#self_ty).to_string(),
            new_name: ident.to_string(),
        };

        visit_item_impl_mut(&mut rewriter, &mut new_impl);

        let new_impl_items = &new_impl.items;

        generated_impls.extend(quote! {
            impl #ident {
                #(#new_impl_items)*
            }
        });
    }

    let expanded = quote! {
        #input_impl
        #generated_impls
    };

    TokenStream::from(expanded)
}

#[proc_macro_attribute]
pub fn generate_extern_fn(args: TokenStream, input: TokenStream) -> TokenStream {
    let input_impl = parse_macro_input!(input as ItemImpl);
    let Args(args) = parse_macro_input!(args as Args);

    let self_ty = &input_impl.self_ty;
    let mut generated_fns = quote! {};

    for arg in args {
        let struct_name_str = format!("{}_{}", quote!(#self_ty).to_string().replace(" ", ""), arg);
        let struct_name = format_ident!("{}", struct_name_str);
        let extern_fn_name = format_ident!("extern_{}_on_msg", struct_name);

        for item in &input_impl.items {
            if let ImplItem::Fn(method) = item {
                if method.sig.ident == "on_msg" {
                    generated_fns.extend(quote! {
                        #[no_mangle]
                        pub extern "C" fn #extern_fn_name(param: u32) {
                            let rt = tokio::runtime::Builder::new_current_thread()
                                .enable_all()
                                .build()
                                .unwrap();
                            rt.block_on(#struct_name::instance().on_msg(param));
                        }

                        #[async_trait::async_trait]
                        impl UListener for #struct_name {
                            async fn on_msg(&self, param: u32) {
                                println!("Received: {}", param);
                            }
                        }
                    });
                }
            }
        }
    }

    let expanded = quote! {
        #input_impl
        #generated_fns
    };

    TokenStream::from(expanded)
}

#[proc_macro]
pub fn generate_extern_fns(input: TokenStream) -> TokenStream {
    let num_fns = syn::parse_macro_input!(input as LitInt).base10_parse::<usize>().unwrap();

    let mut generated_fns = quote! {};

    for i in 0..num_fns {
        let extern_fn_name = format_ident!("extern_on_msg_wrapper_{}", i);
        let fn_name = format_ident!("on_msg_wrapper_{}", i);

        let fn_code = quote! {
            #[no_mangle]
            pub extern "C" fn #extern_fn_name(param: u32) {
                println!("Calling extern function #{}", #i);
                let registry = LISTENER_REGISTRY.lock().unwrap();
                if let Some(listener) = registry.get(&#i) {
                    let listener = Arc::clone(listener);
                    tokio::spawn(async move {
                        #fn_name(listener, param).await;
                    });
                } else {
                    println!("Listener not found for ID {}", #i);
                }
            }

            async fn #fn_name(listener: Arc<dyn UListener>, param: u32) {
                listener.on_msg(param).await;
            }
        };

        generated_fns.extend(fn_code);
    }

    generated_fns.into()
}