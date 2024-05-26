extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{quote, format_ident};
use syn::LitInt;

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
