use heck::ToSnakeCase;
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Ident, Token};
use syn::parse::{Parse, ParseStream};

// Input parser for the macro
struct StateMachineInput {
    struct_name: Ident,
    states: Vec<Ident>,
}

impl Parse for StateMachineInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let struct_name: Ident = input.parse()?;
        input.parse::<Token![;]>()?;
        let states = syn::punctuated::Punctuated::<Ident, Token![,]>::parse_terminated(input)?
            .into_iter()
            .collect();
        Ok(StateMachineInput {
            struct_name,
            states,
        })
    }
}

#[proc_macro]
pub fn state_machine(input: TokenStream) -> TokenStream {
    let StateMachineInput { struct_name, states } = parse_macro_input!(input as StateMachineInput);
    
    let first_state_ident = states.get(0).expect("State machine must have at least one state.");
    
    // Generate state enum
    let enum_name = Ident::new(
        &format!("{}StateEnum", struct_name.to_string()),
        struct_name.span(),
    );
    let enum_variants = states.iter().enumerate().map(|(i, current_state)| {
        if i == 0 {
            quote! {
                #[default]
                #current_state,
            }
        } else {
            quote! {
                #current_state,
            }
        }
    });

    let enum_system_name = Ident::new(
        &format!("{}_enum_trigger_system", struct_name.to_string().to_snake_case()),
        struct_name.span(),
    );

    // Generate trigger systems for each state
    let trigger_systems = states.iter().map(|current_state| {
        let system_name = Ident::new(
            &format!("{}_{}_trigger_system", struct_name.to_string().to_snake_case(), current_state.to_string().to_snake_case()),
            current_state.span(),
        );
        let remove_other_states = states.iter().filter(|&state| state != current_state).map(|state| {
            quote! {
                #enum_name::#state => {
                    bevy_gearbox::commands::StateExitCommandsExt::try_exit_state::<#state>(&mut c, state.clone());
                }
            }
        });

        quote! {
            fn #system_name(
                trigger: Trigger<bevy_gearbox::commands::Transition<#current_state>>,
                mut query: Query<&mut #enum_name, With<#struct_name>>,
                mut commands: Commands,
            ) {
                let Ok(mut state_machine_enum) = query.get_mut(trigger.entity()) else {
                    return;
                };
                let mut c = commands.entity(trigger.entity());
                let state = &trigger.0;
                match *state_machine_enum {
                    #(#remove_other_states)*,
                    #enum_name::#current_state => (),
                }
                *state_machine_enum = #enum_name::#current_state;
            }
        }
    });

    // Add systems to the plugin
    let add_systems = states.iter().map(|state| {
        let system_name = Ident::new(
            &format!("{}_{}_trigger_system", struct_name.to_string().to_snake_case(), state.to_string().to_snake_case()),
            state.span(),
        );
        quote! {
            .add_observer(#system_name)
        }
    });

    // Generate the plugin name
    let plugin_name = Ident::new(&format!("{}Plugin", struct_name), struct_name.span());

    // Generate the expanded code
    let expanded = quote! {
        #[derive(Component, Clone, Debug, Default, Reflect)]
        enum #enum_name {
            #(#enum_variants)*
        }

        fn #enum_system_name(
            trigger: Trigger<OnAdd, #struct_name>,
            mut commands: Commands,
        ) {
            let entity = trigger.entity();
        
            commands.entity(entity).insert(#enum_name::default());
            commands.entity(entity).insert(#first_state_ident::default());
        }

        #(#trigger_systems)*

        pub struct #plugin_name;

        impl Plugin for #plugin_name {
            fn build(&self, app: &mut App) {
                app
                    #(#add_systems)*
                    .add_observer(#enum_system_name);
            }
        }
    };

    TokenStream::from(expanded)
}
