extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{quote, format_ident};
use syn::{LitInt, parse_macro_input};

#[proc_macro]
pub fn generate_extern_fns(input: TokenStream) -> TokenStream {
    let num_fns = parse_macro_input!(input as LitInt).base10_parse::<usize>().unwrap();

    let mut generated_fns = quote! {};
    let mut match_arms = Vec::with_capacity(num_fns);

    for i in 0..num_fns {
        let extern_fn_name = format_ident!("extern_on_msg_wrapper_{}", i);

        let fn_code = quote! {
            #[no_mangle]
            pub extern "C" fn #extern_fn_name(param: u32) {
                call_shared_extern_fn(#i, param);
            }
        };

        generated_fns.extend(fn_code);

        let match_arm = quote! {
            #i => #extern_fn_name,
        };
        match_arms.push(match_arm);
    }

    let expanded = quote! {
        #generated_fns

        fn call_shared_extern_fn(listener_id: usize, param: u32) {
            println!("Calling extern function #{}", listener_id);
            let registry = LISTENER_REGISTRY.lock().unwrap();
            if let Some(listener) = registry.get(&listener_id) {
                let listener = Arc::clone(listener);
                tokio::spawn(async move {
                    shared_async_fn(listener, param).await;
                });
            } else {
                println!("Listener not found for ID {}", listener_id);
            }
        }

        async fn shared_async_fn(listener: Arc<dyn UListener>, param: u32) {
            listener.on_msg(param).await;
        }

        fn get_extern_fn(listener_id: usize) -> extern "C" fn(u32) {
            match listener_id {
                #(#match_arms)*
                _ => panic!("Listener ID out of range"),
            }
        }
    };

    expanded.into()
}
